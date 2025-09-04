use anchor_lang::prelude::*;

//
// ──────────────────────────────────────────────────────────────────────────────
// UserStakeInfo Account
// ──────────────────────────────────────────────────────────────────────────────
//

/// PDA seed string used to derive each user's stake info account.
pub const USER_STAKE_INFO_SEED: &str = "user_stake_info";
pub const ADMIN_STAKE_INFO_SEED: &str = "admin_stake_info";

/// Stores staking and reward-related data for a single user.
///
/// Each staker has one `UserStakeInfo` account, derived from:
/// `USER_STAKE_INFO_SEED + user_pubkey`.
///
/// This account tracks:
/// - The user’s SOL staked.
/// - LXR rewards they’ve claimed or forfeited.
/// - Their base LXR holdings at purchase time (used for pro-rata reward checks).
/// - A reward index checkpoint for calculating pending LXR rewards.
/// - Any explicitly stored pending rewards not yet claimed.
#[account]
#[derive(Default, Debug)]
pub struct UserStakeInfo {
    /// PDA bump for this account.
    pub bump: u8,

    /// Owner (user) to whom this record belongs.
    pub owner: Pubkey,

    /// Total SOL (in lamports) the user has staked.
    pub total_staked_sol: u64,

    /// Total LXR the user has successfully claimed.
    pub total_lxr_claimed: u64,

    /// Total LXR the user has forfeited (sent to treasury due to under-holdings).
    pub total_lxr_forfeited: u64,

    /// The baseline LXR holdings recorded at purchase time.
    /// Used to enforce proportional claiming and forfeiture rules.
    pub base_lxr_holdings: u64,

    /// Reward index checkpoint (global `reward_per_token_lxr_stored`)
    /// at the time of the user’s last update.
    /// Used to calculate incremental rewards owed.
    pub lxr_reward_per_token_completed: u128,

    /// LXR rewards that were calculated but not yet claimed by the user.
    pub lxr_rewards_pending: u64,
    pub blacklisted_sol: u64,
}

impl UserStakeInfo {
    /// Fixed serialized size of the account (for allocation at initialization).
    ///
    /// Breakdown:
    /// - 8: account discriminator
    /// - 1: bump
    /// - 32: owner pubkey
    /// - 8 * 5: five `u64` fields
    /// - 16: one `u128` field
    pub const LEN: usize = 8 + 1 + 32 + 8 * 6 + 16;
}
