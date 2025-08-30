use anchor_lang::prelude::*;

pub const USER_STAKE_INFO_SEED: &str = "user_stake_info";

#[account]
#[derive(Default, Debug)]
pub struct UserStakeInfo {
    pub bump: u8,
    pub owner: Pubkey,
    pub total_staked_sol: u64,
    pub total_lxr_claimed: u64,
    pub total_lxr_forfeited: u64,
    pub base_lxr_holdings: u64,
    pub lxr_reward_per_token_completed: u128,
    pub lxr_rewards_pending: u64,
}

impl UserStakeInfo {
    pub const LEN: usize = 8 + 1 + 32 + 8 * 4 + 1;
}
