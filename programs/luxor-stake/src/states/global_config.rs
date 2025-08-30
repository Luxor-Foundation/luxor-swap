use anchor_lang::prelude::*;

pub const GLOBAL_CONFIG_SEED: &str = "global_config";

#[account]
#[derive(Default, Debug)]
pub struct GlobalConfig {
    pub bump: u8,
    pub admin: Pubkey,
    pub lxr_treasury_vault: Pubkey,
    pub lxr_reward_vault: Pubkey,
    pub stake_account: Pubkey,
    pub bonus_rate: u64,
    pub max_stake_count_to_get_bonus: u64,
    pub min_swap_amount: u64,
    pub max_swap_amount: u64,
    pub fee_treasury_rate: u64,
    pub purchase_enabled: bool,
    pub redeem_enabled: bool,
    pub initial_lxr_allocation_vault: u64,
}

impl GlobalConfig {
    pub const LEN: usize = 8 + 1 + 32 + 8 * 4 + 1;
}
