use crate::{states::*, PRECISION};
use anchor_lang::{prelude::*};
use crate::error::ErrorCode;

//
// ──────────────────────────────────────────────────────────────────────────────
// Blacklist Instruction
// ──────────────────────────────────────────────────────────────────────────────
//

/// Blacklist a user from the protocol.
///
/// This instruction forcibly removes a user’s staked SOL from active participation,
/// forfeits their pending rewards, and transfers their stake accounting into
/// the admin’s `UserStakeInfo`.  
///
/// Effects:
/// - User’s rewards are calculated up to the current reward index and then
///   marked as forfeited.
/// - User’s total staked SOL is reset to `0`, but the same amount is added
///   to the admin’s stake record.
/// - User’s pending rewards are transferred to the admin’s pending rewards.
/// - User’s base LXR holdings are reset to `0`.
/// - An event `UserBlacklisted` is emitted.
#[derive(Accounts)]
pub struct Blacklist<'info> {
    /// Admin (authorized) signer.  
    /// Must be either the current protocol admin stored in `global_config.admin`
    /// or the hardcoded program admin.
    #[account(
        mut,
        constraint = (owner.key() == global_config.admin || owner.key() == crate::admin::id()) @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// Global protocol configuration (holds admin, vaults, stake info ref, etc).
    #[account(
        seeds = [GLOBAL_CONFIG_SEED.as_bytes()],
        bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// The user account being blacklisted.  
    /// Identity key for deriving `user_stake_info`.
    pub user: SystemAccount<'info>,

    /// Per-user stake info for the blacklisted `user`.  
    /// Tracks their staked SOL, rewards, forfeitures, etc.
    #[account(
        mut,
        seeds = [
            USER_STAKE_INFO_SEED.as_bytes(), 
            user.key().as_ref()
        ],
        bump,
    )]
    pub user_stake_info: Account<'info, UserStakeInfo>,

    /// Admin stake info account, where blacklisted stake is reassigned.  
    /// Derived from a fixed seed (`ADMIN_STAKE_INFO_SEED`).
    #[account(
        mut,
        seeds = [
            ADMIN_STAKE_INFO_SEED.as_bytes(),
        ],
        bump,
    )]
    pub admin_stake_info: Account<'info, UserStakeInfo>,

    /// Global stake info account.  
    /// Used to compute reward-per-token deltas for both user and admin.
    #[account(address = global_config.stake_info)]
    pub stake_info: Account<'info, StakeInfo>,

    /// System Program (required by Anchor).  
    /// Not directly used in this instruction.
    pub system_program: Program<'info, System>,
}

/// Instruction: Blacklist a user and reassign their stake to the admin.
///
/// # Steps
/// 1. Compute user’s pending rewards since their last checkpoint:
///    - Add to their `lxr_rewards_pending`.
///    - Then mark all pending rewards as forfeited (`total_lxr_forfeited`).
/// 2. Mark user’s total staked SOL as blacklisted (`blacklisted_sol`) and reset `total_staked_sol = 0`.
/// 3. Compute admin’s pending rewards since their last checkpoint and update.
/// 4. Add user’s stake and pending rewards into the admin’s record.
/// 5. Reset user’s pending rewards and base LXR holdings to `0`.
/// 6. Emit a `UserBlacklisted` event.
pub fn blacklist(ctx: Context<Blacklist>) -> Result<()> {
    let user_stake_info = &mut ctx.accounts.user_stake_info;
    let admin_stake_info = &mut ctx.accounts.admin_stake_info;
    let stake_info = &ctx.accounts.stake_info;

    if admin_stake_info.owner == Pubkey::default() {
       admin_stake_info.owner = ctx.accounts.owner.key();
       admin_stake_info.bump = ctx.bumps.admin_stake_info;
       admin_stake_info.lxr_reward_per_token_completed = stake_info.reward_per_token_lxr_stored;
    }

    // --- 1. Compute user's pending rewards and mark as forfeited ---
    let reward_per_token_lxr_pending_user = stake_info.reward_per_token_lxr_stored
        .checked_sub(user_stake_info.lxr_reward_per_token_completed)
        .unwrap();
    
    let lxr_rewards_to_claim_user = (user_stake_info.total_staked_sol as u128)
        .checked_mul(reward_per_token_lxr_pending_user).unwrap()
        .checked_div(PRECISION).unwrap()
        .checked_div(PRECISION).unwrap() as u64;

    user_stake_info.lxr_rewards_pending = user_stake_info.lxr_rewards_pending
        .checked_add(lxr_rewards_to_claim_user).unwrap();
    user_stake_info.total_lxr_forfeited = user_stake_info.total_lxr_forfeited
        .checked_add(user_stake_info.lxr_rewards_pending).unwrap();

    // Mark SOL as blacklisted
    let sol_blacklisted = user_stake_info.total_staked_sol;
    user_stake_info.blacklisted_sol = user_stake_info.blacklisted_sol
        .checked_add(user_stake_info.total_staked_sol).unwrap();
    user_stake_info.lxr_reward_per_token_completed = stake_info.reward_per_token_lxr_stored;

    // --- 2. Compute admin's pending rewards and add user’s stake ---
    let reward_per_token_lxr_pending_admin = stake_info.reward_per_token_lxr_stored
        .checked_sub(admin_stake_info.lxr_reward_per_token_completed)
        .unwrap();
    let lxr_rewards_to_claim_admin = (admin_stake_info.total_staked_sol as u128)
        .checked_mul(reward_per_token_lxr_pending_admin).unwrap()
        .checked_div(PRECISION).unwrap()
        .checked_div(PRECISION).unwrap() as u64;
    
    admin_stake_info.lxr_rewards_pending = admin_stake_info.lxr_rewards_pending
        .checked_add(lxr_rewards_to_claim_admin).unwrap();
    admin_stake_info.lxr_reward_per_token_completed = stake_info.reward_per_token_lxr_stored;

    // Transfer SOL stake ownership from user → admin
    admin_stake_info.total_staked_sol = admin_stake_info.total_staked_sol
        .checked_add(user_stake_info.total_staked_sol).unwrap();
    user_stake_info.total_staked_sol = 0;

    // Transfer pending rewards from user → admin
    admin_stake_info.lxr_rewards_pending = admin_stake_info.lxr_rewards_pending
        .checked_add(user_stake_info.lxr_rewards_pending).unwrap();
    user_stake_info.lxr_rewards_pending = 0;

    // Reset base holdings for blacklisted user
    user_stake_info.base_lxr_holdings = 0;

    // --- 3. Emit blacklist event ---
    emit!(UserBlacklisted {
        user: ctx.accounts.user.key(),
        sol_blacklisted: sol_blacklisted,
    });

    Ok(())
}