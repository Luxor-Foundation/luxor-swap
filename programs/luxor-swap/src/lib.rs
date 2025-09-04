use anchor_lang::prelude::*;

declare_id!("2mXffWN8gUBsac5YNWaCcKt3Yfhw8DT3yqXJXymQcUnu");

pub mod raydium_cpmm {
    use anchor_lang::prelude::declare_id;
    declare_id!("CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C");
}

pub mod vault_and_lp_mint_auth {
    use anchor_lang::prelude::declare_id;
    declare_id!("GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL");
}

pub mod luxor_pool_state {
    use anchor_lang::prelude::declare_id;
    declare_id!("J7qwfj5wmLNFTN7XYyv1BfQa6xkqo94pohBPPrVEavz7");
}

pub mod admin {
    use anchor_lang::prelude::declare_id;
    declare_id!("B8VmoTgg2arRfw7qQVTYK9GohYeyMEjaCSW6jVPVBUgV");
}

pub mod luxor_mint {
    use anchor_lang::prelude::declare_id;
    declare_id!("EBHC7XpycnQhCd3zq8iWmSuhvpGVyM6krjb6pvwgZ4zE");
}

pub const AUTH_SEED: &str = "stake_and_treasury_auth";
pub const LUXOR_VAULT_SEED: &str = "luxor_vault";
pub const LUXOR_REWARD_VAULT_SEED: &str = "luxor_reward_vault";
pub const SOL_TREASURY_VAULT_SEED: &str = "sol_treasury_vault";
pub const STAKE_ACCOUNT_SEED: &str = "stake";
pub const STAKE_SPLIT_ACCOUNT_SEED: &str = "stake_split";
pub const PRECISION: u128 = 1_000_000_000;

pub mod curve;
pub mod error;
pub mod instructions;
pub mod states;
pub mod utils;

use instructions::*;

#[program]
pub mod luxor_swap {

    use super::*;

    pub fn emergency_withdraw(
        ctx: Context<EmergencyWithdraw>,
        param: u8,
        value: u64,
    ) -> Result<()> {
        instructions::emergency_withdraw(ctx, param, value)
    }

    pub fn update_config(ctx: Context<UpdateConfig>, param: u8, value: u64) -> Result<()> {
        instructions::update_config(ctx, param, value)
    }

    pub fn buyback(ctx: Context<Buyback>) -> Result<()> {
        instructions::buyback(ctx)
    }

    pub fn redeem(ctx: Context<Redeem>) -> Result<()> {
        instructions::redeem(ctx)
    }

    pub fn blacklist(ctx: Context<Blacklist>) -> Result<()> {
        instructions::blacklist(ctx)
    }

    pub fn purchase(
        ctx: Context<Purchase>,
        lxr_to_purchase: u64,
        max_sol_amount: u64,
    ) -> Result<()> {
        instructions::purchase(ctx, lxr_to_purchase, max_sol_amount)
    }

    pub fn manual_purchase(
        ctx: Context<ManualPurchase>,
        lxr_purchased: u64,
        sol_spent: u64,
    ) -> Result<()> {
        instructions::manual_purchase(ctx, lxr_purchased, sol_spent)
    }

    pub fn initialise_configs(
        ctx: Context<InitialiseConfigs>,
        admin: Pubkey,
        vote_account: Pubkey,
        bonus_rate: u64,
        max_stake_count_to_get_bonus: u64,
        min_swap_amount: u64,
        max_swap_amount: u64,
        fee_treasury_rate: u64,
        purchase_enabled: bool,
        redeem_enabled: bool,
        initial_lxr_allocation_vault: u64,
    ) -> Result<()> {
        instructions::initialise_configs(
            ctx,
            admin,
            vote_account,
            bonus_rate,
            max_stake_count_to_get_bonus,
            min_swap_amount,
            max_swap_amount,
            fee_treasury_rate,
            purchase_enabled,
            redeem_enabled,
            initial_lxr_allocation_vault,
        )
    }
}
