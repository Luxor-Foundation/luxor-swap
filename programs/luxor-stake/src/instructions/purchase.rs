use crate::curve::{CurveCalculator, FEE_RATE_DENOMINATOR_VALUE};
use crate::error::ErrorCode;
use crate::utils::transfer_from_pool_vault_to_user;
use crate::{states::*, PRECISION, STAKE_ACCOUNT_SEED};
use anchor_lang::{prelude::*, solana_program};
use anchor_lang::solana_program::program::{invoke, invoke_signed};
use anchor_lang::solana_program::stake::instruction as stake_ix;
use anchor_lang::solana_program::system_instruction::transfer;
use anchor_lang::solana_program::{stake, sysvar};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

/// Initializes global protocol configuration, mints the vault ATA,
/// and **creates + initializes** a Stake account at a PDA derived from `STAKE_ACCOUNT_SEED`.
///
/// The Stake account is created with **owner = Stake program** and funded with **rent-exempt minimum only**.
/// It is initialized with `staker` and `withdrawer` set to the program `authority` PDA.
/// No delegation happens here (can be done later).
#[derive(Accounts)]
pub struct Purchase<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    pub global_config: Account<'info, GlobalConfig>,

    pub luxor_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        seeds = [
            USER_STAKE_INFO_SEED.as_bytes(), 
            owner.key().as_ref()
        ],
        bump,
        payer = owner,
        space = UserStakeInfo::LEN
    )]
    pub user_stake_info: Account<'info, UserStakeInfo>,

    pub stake_info: Account<'info, StakeInfo>,

    /// CHECK: Authority PDA
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    #[account(
        constraint = luxor_mint.key() == crate::luxor_mint::id() @ ErrorCode::InvalidLuxorMint,
        mint::token_program = token_program,
    )]
    pub luxor_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK Owner lp tokan account
    #[account(
        mut,
        token::mint = luxor_mint,
        token::authority = owner,
        token::token_program = token_program,  
    )]
    pub owner_lxr_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: Stake account PDA
    #[account(
        mut,
        seeds = [STAKE_ACCOUNT_SEED.as_bytes()],
        bump
    )]
    pub stake_pda: UncheckedAccount<'info>,

    /// CHECK: Validator’s vote account
    pub vote_account: UncheckedAccount<'info>,

    /// CHECK: Stake program
    #[account(constraint = stake_program.key() == stake::program::ID)]
    pub stake_program: UncheckedAccount<'info>,

    /// CHECK: Clock sysvar
    #[account(address = sysvar::clock::ID)]
    pub clock: UncheckedAccount<'info>,
    /// CHECK: Stake history sysvar
    #[account(address = sysvar::stake_history::ID)]
    pub stake_history: UncheckedAccount<'info>,
    /// CHECK: Stake config account (fixed program address)
    #[account(address = solana_program::stake::config::ID)]
    pub stake_config: UncheckedAccount<'info>,

    /// CHECK: Raydium pool state account
    #[account(
        owner = crate::raydium_cpmm::id()
    )]
    pub pool_state: UncheckedAccount<'info>,

    /// The address that holds pool tokens for token_0
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    pub token_1_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Token program (interface)
    pub token_program: Interface<'info, TokenInterface>,

    /// System program
    pub system_program: Program<'info, System>,
}

pub fn purchase(ctx: Context<Purchase>, lxr_to_purchase: u64, max_sol_amount: u64) -> Result<()> {
    require_gt!(lxr_to_purchase, 0);
    let pool_state_info = &ctx.accounts.pool_state;
    let pool_state = PoolState::try_deserialize(&mut &pool_state_info.data.borrow()[..])?;
    let amount_out_with_transfer_fee = lxr_to_purchase;

    let SwapParams {
        trade_direction : _,
        total_input_token_amount,
        total_output_token_amount,
        token_0_price_x64: _,
        token_1_price_x64: _,
        is_creator_fee_on_input,
    } = pool_state.get_swap_params(
        ctx.accounts.token_0_vault.key(),
        ctx.accounts.token_1_vault.key(),
        ctx.accounts.token_0_vault.amount,
        ctx.accounts.token_1_vault.amount,
    )?;

    let constant_before = u128::from(total_input_token_amount)
        .checked_mul(u128::from(total_output_token_amount))
        .unwrap();

    let creator_fee_rate =
        pool_state.adjust_creator_fee_rate(500);

    let result = CurveCalculator::swap_base_output(
        u128::from(amount_out_with_transfer_fee),
        u128::from(total_input_token_amount),
        u128::from(total_output_token_amount),
        2500,
        creator_fee_rate,
        120000,
        40000,
        is_creator_fee_on_input,
    )
    .ok_or(ErrorCode::ZeroTradingTokens)?;

    let constant_after = u128::from(result.new_input_vault_amount)
        .checked_mul(u128::from(result.new_output_vault_amount))
        .unwrap();
    
    require_eq!(
        u64::try_from(result.output_amount).unwrap(),
        amount_out_with_transfer_fee
    );
    
    require_gte!(constant_after, constant_before);
    let mut total_sol_needed = u64::try_from(result.input_amount).unwrap();
    
    let stake_info = &mut ctx.accounts.stake_info;
    let user_stake_info = &mut ctx.accounts.user_stake_info;
    let global_config = &ctx.accounts.global_config;

    if stake_info.total_stake_count + 1  <= global_config.max_stake_count_to_get_bonus {
       total_sol_needed = total_sol_needed
        .checked_sub(
            total_sol_needed
        .checked_mul(global_config.bonus_rate).unwrap()
        .checked_div(FEE_RATE_DENOMINATOR_VALUE).unwrap()
       ).unwrap();
    } else {
        total_sol_needed = u128::from(total_sol_needed)
        .checked_mul(ctx.accounts.luxor_vault.amount as u128).unwrap()
        .checked_div(global_config.initial_lxr_allocation_vault as u128).unwrap() as u64;
    }
    require_gte!(max_sol_amount, total_sol_needed);
    if ctx.accounts.stake_pda.lamports() > stake_info.last_tracked_sol_balance {
        let rewards_accured = ctx.accounts.stake_pda.lamports()
            .checked_sub(stake_info.last_tracked_sol_balance).unwrap();
        stake_info.total_sol_rewards_accrued = stake_info.total_sol_rewards_accrued
            .checked_add(rewards_accured).unwrap();
        stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
        stake_info.reward_per_token_sol_stored = stake_info.reward_per_token_sol_stored.checked_add(
            (rewards_accured as u128)
            .checked_mul(PRECISION).unwrap()
            .checked_div(stake_info.total_staked_sol as u128).unwrap()
        ).unwrap();
    }
    let ix = transfer(&ctx.accounts.owner.key(), &ctx.accounts.stake_pda.key(), total_sol_needed);
    invoke(
    &ix,
    &[
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.stake_pda.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
    ],)?;

    let stake_key = ctx.accounts.stake_pda.key();
    let vote_key  = ctx.accounts.vote_account.key();
    let auth_key  = ctx.accounts.authority.key();

    // Build CPI ix
    let ix = stake_ix::delegate_stake(&stake_key, &auth_key, &vote_key);

    let account_infos = &[
        ctx.accounts.stake_pda.to_account_info(),
        ctx.accounts.vote_account.to_account_info(),
        ctx.accounts.clock.to_account_info(),
        ctx.accounts.stake_history.to_account_info(),
        ctx.accounts.stake_config.to_account_info(),
        ctx.accounts.authority.to_account_info(),
    ];

    // PDA seeds (authority is a PDA, not a real signer)
    let auth_bump = ctx.bumps.authority;
    let seeds: &[&[u8]] = &[crate::AUTH_SEED.as_bytes(), &[auth_bump]];

    invoke_signed(&ix, account_infos, &[seeds])?;

    // stake info updates
    stake_info.total_staked_sol = stake_info.total_staked_sol
        .checked_add(total_sol_needed).unwrap();
    stake_info.total_stake_count = stake_info.total_stake_count
        .checked_add(1).unwrap();
    stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;
    stake_info.last_update_timestamp = block_timestamp;

    // user stake info updates
    if user_stake_info.owner == Pubkey::default() {
        user_stake_info.owner = ctx.accounts.owner.key();
        user_stake_info.bump = ctx.bumps.user_stake_info;
    }
    user_stake_info.total_staked_sol = user_stake_info.total_staked_sol
        .checked_add(total_sol_needed).unwrap();
    user_stake_info.base_lxr_holdings = user_stake_info.base_lxr_holdings
        .checked_add(lxr_to_purchase).unwrap();

    // transfer lxr from vault to user

    transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.luxor_vault.to_account_info(),
        ctx.accounts.owner_lxr_token.to_account_info(),
        ctx.accounts.luxor_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        lxr_to_purchase,
        ctx.accounts.luxor_mint.decimals,
        &[&[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]]],
    )?;
    // emit event

    Ok(())
}
