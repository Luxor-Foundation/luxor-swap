use crate::{states::*, PRECISION, STAKE_ACCOUNT_SEED};
use anchor_lang::{prelude::*, solana_program};
use anchor_lang::solana_program::program::{invoke, invoke_signed};
use anchor_lang::solana_program::stake::instruction as stake_ix;
use anchor_lang::solana_program::system_instruction::transfer;
use anchor_lang::solana_program::{stake, sysvar};

/// Initializes global protocol configuration, mints the vault ATA,
/// and **creates + initializes** a Stake account at a PDA derived from `STAKE_ACCOUNT_SEED`.
///
/// The Stake account is created with **owner = Stake program** and funded with **rent-exempt minimum only**.
/// It is initialized with `staker` and `withdrawer` set to the program `authority` PDA.
/// No delegation happens here (can be done later).
#[derive(Accounts)]
pub struct ManualPurchase<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

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

    /// System program
    pub system_program: Program<'info, System>,
}

pub fn manual_purchase(ctx: Context<ManualPurchase>, lxr_purchased: u64, sol_spent: u64) -> Result<()> {
    
    let stake_info = &mut ctx.accounts.stake_info;
    let user_stake_info = &mut ctx.accounts.user_stake_info;
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
    let ix = transfer(&ctx.accounts.owner.key(), &ctx.accounts.stake_pda.key(), sol_spent);
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
        .checked_add(sol_spent).unwrap();
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
        .checked_add(sol_spent).unwrap();
    user_stake_info.base_lxr_holdings = user_stake_info.base_lxr_holdings
        .checked_add(lxr_purchased).unwrap();
    
    // emit event

    Ok(())
}
