use anchor_lang::prelude::*;

pub const STAKE_INFO_SEED: &str = "stake_info";

#[account]
#[derive(Default, Debug)]
pub struct StakeInfo {
    pub bump: u8,
    pub total_staked_sol: u64,
    pub total_stake_count: u64,
    pub total_sol_rewards_accrued: u64,
    pub last_tracked_sol_balance: u64,
    pub reward_per_token_sol_stored: u128,
    pub total_luxor_rewards_accrued: u64,
    pub total_sol_used_for_buyback: u64,
    pub last_update_timestamp: u64,
    pub last_buyback_timestamp: u64,
    pub reward_per_token_lxr_stored: u128,
    pub total_lxr_claimed: u64,
    pub total_lxr_forfeited: u64,
}

impl StakeInfo {
    pub const LEN: usize = 8 + 1 + 8 * 8 + 16 * 3;
}
