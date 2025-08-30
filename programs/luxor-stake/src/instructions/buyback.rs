use crate::curve::CurveCalculator;
use crate::error::ErrorCode;
use crate::states::*;
use crate::PRECISION;
use crate::STAKE_ACCOUNT_SEED;
use anchor_lang::prelude::borsh::BorshDeserialize;
use anchor_lang::prelude::borsh::BorshSerialize;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::invoke;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::system_instruction::transfer;
use anchor_spl::token::spl_token;
use anchor_spl::token::spl_token::instruction::sync_native;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, TokenAccount};

#[derive(BorshSerialize, BorshDeserialize)]
pub struct SwapBaseInput {
    amount_in: u64,
    minimum_amount_out: u64,
}

#[derive(Accounts)]
pub struct Buyback<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    pub stake_info: Account<'info, StakeInfo>,

    /// CHECK: This is the PDA that holds staked SOL and receives rewards
    #[account(
        mut,
        seeds = [STAKE_ACCOUNT_SEED.as_bytes()],
        bump
    )]
    pub stake_pda: UncheckedAccount<'info>,

    /// The owner's token account for receive token_0
    #[account(
        mut,
        token::mint = token_0_vault.mint,
        token::authority = owner
    )]
    pub token_0_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The owner's token account for receive token_1
    #[account(
        mut,
        token::mint = token_1_vault.mint,
        token::authority = owner
    )]
    pub token_1_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(mut)]
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(mut)]
    pub token_1_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// token Program
    pub token_program: Program<'info, Token>,

    /// The mint of token_0 vault
    #[account(
        address = token_0_vault.mint
    )]
    pub vault_0_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of token_1 vault
    #[account(
        address = token_1_vault.mint
    )]
    pub vault_1_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: Raydium pool state account
    #[account(
        owner = crate::raydium_cpmm::id()
    )]
    pub pool_state: UncheckedAccount<'info>,

    /// CHECK: Raydium authority account
    #[account(
        address = crate::vault_and_lp_mint_auth::id()
    )]
    pub raydium_authority: UncheckedAccount<'info>,
    /// CHECK: Raydium amm config account
    pub amm_config: UncheckedAccount<'info>,
    /// CHECK: Raydium observation state account
    pub observation_state: UncheckedAccount<'info>,

    /// CHECK: Raydium CPMM program
    #[account(
        mut,
        address = crate::raydium_cpmm::id()
    )]
    pub raydium_cpmm_program: AccountInfo<'info>,

    /// System program
    pub system_program: Program<'info, System>,
}

pub fn buyback(ctx: Context<Buyback>) -> Result<()> {
    let stake_info = &mut ctx.accounts.stake_info;

    if ctx.accounts.stake_pda.lamports() > stake_info.last_tracked_sol_balance {
        let rewards_accured = ctx
            .accounts
            .stake_pda
            .lamports()
            .checked_sub(stake_info.last_tracked_sol_balance)
            .unwrap();
        stake_info.total_sol_rewards_accrued = stake_info
            .total_sol_rewards_accrued
            .checked_add(rewards_accured)
            .unwrap();
        stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
        stake_info.reward_per_token_sol_stored = stake_info
            .reward_per_token_sol_stored
            .checked_add(
                (rewards_accured as u128)
                    .checked_mul(PRECISION)
                    .unwrap()
                    .checked_div(stake_info.total_staked_sol as u128)
                    .unwrap(),
            )
            .unwrap();
    }
    // calculate the amount amount of sol use to buyback
    let reward_available_to_buyback = stake_info
        .total_sol_rewards_accrued
        .checked_sub(stake_info.total_sol_used_for_buyback)
        .unwrap();

    let ix = transfer(
        &ctx.accounts.stake_pda.key(),
        &ctx.accounts.token_0_account.key(),
        reward_available_to_buyback,
    );

    let stake_account = ctx.accounts.stake_pda.to_account_info();
    let recipient_ai = ctx.accounts.token_0_account.to_account_info();
    let system_program = ctx.accounts.system_program.to_account_info();
    let token_program = ctx.accounts.token_program.to_account_info();

    let bump = ctx.bumps.stake_pda;
    let seeds: &[&[u8]] = &[STAKE_ACCOUNT_SEED.as_bytes(), &[bump]];

    invoke_signed(
        &ix,
        &[stake_account, recipient_ai.clone(), system_program],
        &[seeds],
    )?;

    let sync_ix = sync_native(&spl_token::id(), &ctx.accounts.token_0_account.key())?;
    invoke(&sync_ix, &[recipient_ai, token_program.clone()])?;
    // calculate amount of lxr received after buyback
    let actual_amount_in = reward_available_to_buyback;
    require_gt!(actual_amount_in, 0);
    let pool_state_info = &ctx.accounts.pool_state;
    let pool_state = PoolState::try_deserialize(&mut &pool_state_info.data.borrow()[..])?;
    let SwapParams {
        trade_direction: _,
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
    let creator_fee_rate = pool_state.adjust_creator_fee_rate(500);

    let result = CurveCalculator::swap_base_input(
        u128::from(actual_amount_in),
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
        u64::try_from(result.input_amount).unwrap(),
        actual_amount_in
    );

    require_gte!(constant_after, constant_before);
    let lxr_bought = u64::try_from(result.output_amount).unwrap();

    // update stake info
    stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
    stake_info.total_luxor_rewards_accrued = stake_info
        .total_luxor_rewards_accrued
        .checked_add(lxr_bought)
        .unwrap();
    stake_info.total_sol_used_for_buyback = stake_info
        .total_sol_used_for_buyback
        .checked_add(actual_amount_in)
        .unwrap();
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;
    stake_info.last_update_timestamp = block_timestamp;
    stake_info.last_buyback_timestamp = block_timestamp;
    stake_info.reward_per_token_lxr_stored = stake_info
        .reward_per_token_lxr_stored
        .checked_add(
            (lxr_bought as u128)
                .checked_mul(PRECISION)
                .unwrap()
                .checked_div(stake_info.total_staked_sol as u128)
                .unwrap(),
        )
        .unwrap();

    // actual purchase from raydium pool
    let params = SwapBaseInput {
        amount_in: actual_amount_in,
        minimum_amount_out: 0,
    };

    let discriminator =
        anchor_lang::solana_program::hash::hash(b"global:swap_base_input").to_bytes()[..8].to_vec();
    let mut data = discriminator;
    data.extend(params.try_to_vec()?);

    let payer = ctx.accounts.owner.key();
    let raydium_authority = ctx.accounts.raydium_authority.key();
    let amm_config = ctx.accounts.amm_config.key();
    let pool_state = ctx.accounts.pool_state.key();
    let input_token_account = ctx.accounts.token_0_account.key();
    let output_token_account = ctx.accounts.token_1_account.key();
    let input_vault = ctx.accounts.token_0_vault.key();
    let output_vault = ctx.accounts.token_1_vault.key();
    let input_output_token_program = ctx.accounts.token_program.key();
    let input_token_mint = ctx.accounts.vault_0_mint.key();
    let output_token_mint = ctx.accounts.vault_1_mint.key();
    let observation_state = ctx.accounts.observation_state.key();

    let accounts = vec![
        AccountMeta::new(payer, true),
        AccountMeta::new_readonly(raydium_authority, false),
        AccountMeta::new_readonly(amm_config, false),
        AccountMeta::new(pool_state, false),
        AccountMeta::new(input_token_account, false),
        AccountMeta::new(output_token_account, false),
        AccountMeta::new(input_vault, false),
        AccountMeta::new(output_vault, false),
        AccountMeta::new_readonly(input_output_token_program, false),
        AccountMeta::new_readonly(input_output_token_program, false),
        AccountMeta::new_readonly(input_token_mint, false),
        AccountMeta::new_readonly(output_token_mint, false),
        AccountMeta::new(observation_state, false),
    ];

    let ix = Instruction {
        program_id: crate::raydium_cpmm::id(),
        accounts,
        data,
    };

    let accounts = Box::new(vec![
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.raydium_authority.to_account_info(),
        ctx.accounts.amm_config.to_account_info(),
        ctx.accounts.pool_state.to_account_info(),
        ctx.accounts.token_0_account.to_account_info(),
        ctx.accounts.token_1_account.to_account_info(),
        ctx.accounts.token_0_vault.to_account_info(),
        ctx.accounts.token_1_vault.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.vault_0_mint.to_account_info(),
        ctx.accounts.vault_1_mint.to_account_info(),
        ctx.accounts.observation_state.to_account_info(),
    ]);

    invoke(&ix, &*accounts)?;

    Ok(())
}
