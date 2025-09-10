use crate::instructions::load_stake_state;
use crate::{states::*, PRECISION};
use anchor_lang::solana_program::stake::state::StakeStateV2;
use anchor_lang::{prelude::*, solana_program};
use anchor_lang::solana_program::program::{invoke, invoke_signed};
use anchor_lang::solana_program::stake::instruction as stake_ix;
use anchor_lang::solana_program::system_instruction::transfer;
use anchor_lang::solana_program::{stake};
use crate::error::ErrorCode;

/// Admin-only path to record a purchase for a given `user` by directly
/// specifying how much LXR they obtained (`lxr_purchased`) and how much
/// SOL was spent (`sol_spent`). Unlike the regular `purchase` flow, this
/// instruction **does not price via a pool/curve**—it trusts the admin’s
/// inputs and simply:
///
/// 1) Accrues any pending SOL rewards on the stake PDA into `stake_info`.
/// 2) Transfers `sol_spent` SOL from the admin to the stake PDA.
/// 3) Delegates the new stake to the configured validator vote account.
/// 4) Updates global and per-user staking totals and reward indices.
/// 5) Emits a `ManualLxrPurchased` event for off-chain consumers.
///
/// Intended uses include backfills, adjustments, or manual settlements
/// where external pricing/settlement occurred and must be reflected on-chain.
#[derive(Accounts)]
pub struct ManualPurchase<'info> {
    /// Admin (authorized) signer. Must be either the current protocol admin
    /// stored in `global_config.admin` or the hardcoded program admin.
    #[account(
        mut,
        constraint = (owner.key() == global_config.admin || owner.key() == crate::admin::id()) @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// Global configuration for the protocol.
    #[account(
        seeds = [GLOBAL_CONFIG_SEED.as_bytes()],
        bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// The user account for whom this manual purchase is being recorded.
    /// Used as the identity in `user_stake_info` and the event.
    pub user: SystemAccount<'info>,

    /// Per-user stake info for the target `user`. Lazily initialized if missing.
    #[account(
        init_if_needed,
        seeds = [
            USER_STAKE_INFO_SEED.as_bytes(), 
            user.key().as_ref()
        ],
        bump,
        payer = owner,
        space = UserStakeInfo::LEN
    )]
    pub user_stake_info: Account<'info, UserStakeInfo>,

    /// Global stake metrics and reward indices.
    #[account(
        mut,
        address = global_config.stake_info,
    )]
    pub stake_info: Account<'info, StakeInfo>,

    /// Program authority PDA that acts as stake authority (staker/withdrawer).
    ///
    /// CHECK: PDA derivation is enforced by seeds; used only as a signer PDA.
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// Stake account (PDA) which holds the SOL and is delegated to the validator.
    ///
    /// CHECK: Address comes from config; ownership by Stake program enforced elsewhere.
    #[account(
        mut,
        address = global_config.stake_account
    )]
    pub stake_pda: UncheckedAccount<'info>,

    /// Validator’s vote account to which stake is delegated.
    ///
    /// CHECK: Pinned by config and validated by Stake CPI.
    #[account(address = global_config.vote_account)]
    pub vote_account: UncheckedAccount<'info>,

    /// Stake program for CPI.
    ///
    /// CHECK: Program ID only.
    #[account(address = stake::program::ID)]
    pub stake_program: UncheckedAccount<'info>,

    /// Clock sysvar required by `delegate_stake`.
    ///
    /// CHECK: Program ID only.
    pub clock:  Sysvar<'info, Clock>,

    /// Stake history sysvar required by `delegate_stake`.
    ///
    /// CHECK: Program ID only.
    pub stake_history: Sysvar<'info, StakeHistory>,

    /// Stake config account (fixed program address) required by `delegate_stake`.
    ///
    /// CHECK: Program ID only.
    #[account(address = solana_program::stake::config::ID)]
    pub stake_config: UncheckedAccount<'info>,

    /// System Program used for SOL transfer (owner → stake_pda).
    pub system_program: Program<'info, System>,
}

/// Records a manual LXR purchase and delegates the corresponding SOL as stake.
///
/// # Parameters
/// - `lxr_purchased`: Amount of LXR credited to the `user` (base units).
/// - `sol_spent`: Amount of SOL provided (from `owner`) and staked on behalf of the `user`.
///
/// # Behavior
/// - Accrues any newly observed SOL rewards on the stake PDA.
/// - Transfers `sol_spent` from `owner` to `stake_pda`.
/// - Delegates stake to `vote_account` using `authority` PDA via CPI.
/// - Updates global counters (`total_staked_sol`, `total_stake_count`, etc.)
///   and the user’s aggregates (`total_staked_sol`, `base_lxr_holdings`).
/// - Emits `ManualLxrPurchased { purchaser, sol_amount, lxr_amount }`.
///
/// # Notes
/// - No pricing is computed here—caller must ensure `lxr_purchased` and `sol_spent`
///   reflect an externally agreed settlement.
/// - Assumes `stake_pda` is already initialized as a Stake account with `authority` set.
pub fn manual_purchase(ctx: Context<ManualPurchase>, lxr_purchased: u64, sol_spent: u64) -> Result<()> {
    
    let stake_info = &mut ctx.accounts.stake_info;
    let user_stake_info = &mut ctx.accounts.user_stake_info;

    let stake_pda_ai = ctx.accounts.stake_pda.to_account_info();
    let stake_pda_state = load_stake_state(&stake_pda_ai)?;
    let clock = &*ctx.accounts.clock;               
    let stake_history = &*ctx.accounts.stake_history;
    let mut to_delegate = true;
    match stake_pda_state {
        StakeStateV2::Stake(_,stake , _) => {
            let status = stake.delegation.stake_activating_and_deactivating(clock.epoch, stake_history, None);
            msg!("status {:#?}",status);
            if status.effective > 0 {
               to_delegate = false;
            }

        }
        StakeStateV2::Initialized(_) => {
          msg!("Stake account is in Initialized state, using it for delegation");
        }
        _ => {}
    }


    // --- Accrue any newly observed SOL rewards on the stake PDA ---
    if ctx.accounts.stake_pda.lamports() > stake_info.last_tracked_sol_balance {
        let rewards_accured = ctx.accounts.stake_pda.lamports()
            .checked_sub(stake_info.last_tracked_sol_balance).unwrap();
        stake_info.total_sol_rewards_accrued = stake_info.total_sol_rewards_accrued
            .checked_add(rewards_accured).unwrap();
        stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
    }

    // --- Transfer SOL from admin to the stake PDA (fund new stake) ---
    let ix = transfer(&ctx.accounts.owner.key(), &ctx.accounts.stake_pda.key(), sol_spent);
    invoke(
    &ix,
    &[
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.stake_pda.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
    ],)?;

    // --- Delegate the newly funded stake to the configured validator ---
    let stake_key = ctx.accounts.stake_pda.key();
    let vote_key  = ctx.accounts.vote_account.key();
    let auth_key  = ctx.accounts.authority.key();

    if to_delegate {
        // Build CPI instruction to Stake program.
        let ix = stake_ix::delegate_stake(&stake_key, &auth_key, &vote_key);

        let account_infos = &[
            ctx.accounts.stake_pda.to_account_info(),
            ctx.accounts.vote_account.to_account_info(),
            ctx.accounts.clock.to_account_info(),
            ctx.accounts.stake_history.to_account_info(),
            ctx.accounts.stake_config.to_account_info(),
            ctx.accounts.authority.to_account_info(),
        ];

        // PDA signer seeds for `authority`.
        let auth_bump = ctx.bumps.authority;
        let seeds: &[&[u8]] = &[crate::AUTH_SEED.as_bytes(), &[auth_bump]];

        invoke_signed(&ix, account_infos, &[seeds])?;
    }

   

    // --- Global stake info updates ---
    stake_info.total_staked_sol = stake_info.total_staked_sol
        .checked_add(sol_spent).unwrap();
    stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;
    stake_info.last_update_timestamp = block_timestamp;

    // --- User stake info updates (lazy init + aggregates) ---
    if user_stake_info.owner == Pubkey::default() {
        user_stake_info.owner = ctx.accounts.user.key();
        user_stake_info.bump = ctx.bumps.user_stake_info;
        user_stake_info.lxr_reward_per_token_completed = stake_info.reward_per_token_lxr_stored;
    } else {
        let reward_per_token_lxr_pending_user = stake_info.reward_per_token_lxr_stored
        .checked_sub(user_stake_info.lxr_reward_per_token_completed)
        .unwrap();

        let lxr_rewards_to_claim_user = (user_stake_info.total_staked_sol as u128)
        .checked_mul(reward_per_token_lxr_pending_user).unwrap()
        .checked_div(PRECISION).unwrap() as u64;

        user_stake_info.lxr_rewards_pending = user_stake_info.lxr_rewards_pending
        .checked_add(lxr_rewards_to_claim_user).unwrap();
        user_stake_info.lxr_reward_per_token_completed = stake_info.reward_per_token_lxr_stored;
    }
    
    user_stake_info.total_staked_sol = user_stake_info.total_staked_sol
        .checked_add(sol_spent).unwrap();
    user_stake_info.base_lxr_holdings = user_stake_info.base_lxr_holdings
        .checked_add(lxr_purchased).unwrap();
    
    // --- Emit event for indexers/UX ---
    emit!(ManualLxrPurchased{
        purchaser: ctx.accounts.user.key(),
        sol_amount: sol_spent,
        lxr_amount: lxr_purchased,
    });

    Ok(())
}