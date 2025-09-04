use anchor_lang::prelude::*;

//
// ──────────────────────────────────────────────────────────────────────────────
// StakeInfo Account
// ──────────────────────────────────────────────────────────────────────────────
//

/// PDA seed string used to derive the global staking info account.
pub const STAKE_INFO_SEED: &str = "stake_info";

/// Stores aggregated statistics and reward indices for the entire protocol.
///
/// This account tracks:
/// - Global staking totals (amount of SOL, number of stakes).
/// - Accrued rewards in both SOL and LXR.
/// - Reward-per-token indices (for distributing rewards proportionally).
/// - Timestamps of last updates and buybacks.
/// - Totals of claimed and forfeited LXR.
///
/// Each user has their own `UserStakeInfo` for individual accounting, but all
/// global reward math derives from this account.
#[account]
#[derive(Default, Debug)]
pub struct StakeInfo {
    /// PDA bump for this account.
    pub bump: u8,

    /// Total SOL (in lamports) staked across all users.
    pub total_staked_sol: u64,

    /// Total number of distinct stakes made (used for early-bird bonus logic).
    pub total_stake_count: u64,

    /// Cumulative SOL rewards accrued by the stake PDA since inception.
    pub total_sol_rewards_accrued: u64,

    /// Last observed SOL balance of the stake PDA, used to detect newly accrued rewards.
    pub last_tracked_sol_balance: u64,

    /// Global reward index for SOL-denominated rewards, scaled by PRECISION.
    /// Used to calculate each user's share of accrued SOL rewards.
    pub reward_per_token_sol_stored: u128,

    /// Cumulative amount of LXR bought back from rewards and accrued globally.
    pub total_luxor_rewards_accrued: u64,

    /// Cumulative amount of SOL used for buybacks (subset of total accrued SOL).
    pub total_sol_used_for_buyback: u64,

    /// Last UNIX timestamp (seconds) when any update was made to this account.
    pub last_update_timestamp: u64,

    /// Last UNIX timestamp (seconds) when a buyback was executed.
    pub last_buyback_timestamp: u64,

    /// Global reward index for LXR-denominated rewards, scaled by PRECISION.
    /// Used to calculate each user’s pending LXR entitlement.
    pub reward_per_token_lxr_stored: u128,

    /// Total LXR claimed by all users (sum of successful redemptions).
    pub total_lxr_claimed: u64,

    /// Total LXR forfeited by users (sent to treasury due to under-holdings).
    pub total_lxr_forfeited: u64,

    pub buyback_count: u64,
    pub buyback_requested: bool,
}

impl StakeInfo {
    /// Fixed serialized size of the account (for allocation at initialization).
    ///
    /// Breakdown:
    /// - 8: account discriminator
    /// - 1: bump
    /// - 8 * 10: ten `u64` fields
    /// - 16 * 2: two `u128` fields
    pub const LEN: usize = 8 + 1 + 8 * 11 + 16 * 2 + 1;
}
