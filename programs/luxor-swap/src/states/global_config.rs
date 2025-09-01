use anchor_lang::prelude::*;

//
// ──────────────────────────────────────────────────────────────────────────────
// Global Configuration Account
// ──────────────────────────────────────────────────────────────────────────────
//

/// PDA seed string used to derive the global configuration account.
pub const GLOBAL_CONFIG_SEED: &str = "global_config";

/// Stores all protocol-wide configuration parameters and fixed vault addresses.
///
/// This account is created once at initialization (`InitialiseConfigs`) and is
/// referenced by nearly all instructions. It holds both **static addresses**
/// (vaults, stake PDA, vote account) and **tunable parameters** (fees, limits).
#[account]
#[derive(Default, Debug)]
pub struct GlobalConfig {
    /// PDA bump for this account (for seed derivation).
    pub bump: u8,

    /// Current admin of the protocol (authorized to update config).
    pub admin: Pubkey,

    /// Program-owned token vault holding LXR treasury (fees, forfeitures).
    pub lxr_treasury_vault: Pubkey,

    /// Program-owned token vault holding LXR rewards (pays user redemptions).
    pub lxr_reward_vault: Pubkey,

    /// Program-owned WSOL vault serving as the SOL treasury (receives fees).
    pub sol_treasury_vault: Pubkey,

    /// PDA stake account (owned by Stake program, delegated to `vote_account`).
    pub stake_account: Pubkey,

    /// Validator vote account to which protocol stake is delegated.
    pub vote_account: Pubkey,

    /// Account holding aggregate stake statistics and reward indices.
    pub stake_info: Pubkey,

    /// Bonus rate applied to purchases while total stake count ≤ threshold.
    /// Expressed as numerator with denominator `FEE_RATE_DENOMINATOR_VALUE`.
    pub bonus_rate: u64,

    /// Maximum stake count threshold at which bonus rate still applies.
    pub max_stake_count_to_get_bonus: u64,

    /// Minimum LXR amount permitted in a swap or purchase.
    pub min_swap_amount: u64,

    /// Maximum LXR amount permitted in a swap or purchase.
    pub max_swap_amount: u64,

    /// Fee rate applied to treasury (for buybacks and related flows).
    pub fee_treasury_rate: u64,

    /// Global switch: if `false`, purchasing is disabled.
    pub purchase_enabled: bool,

    /// Global switch: if `false`, redemption is disabled.
    pub redeem_enabled: bool,

    /// Initial LXR allocation used as a reference value for scaling purchase pricing.
    pub initial_lxr_allocation_vault: u64,
}

impl GlobalConfig {
    /// Fixed serialized size of the account (for allocation at initialization).
    ///
    /// Breakdown:
    /// - 8: account discriminator
    /// - 1: bump
    /// - 32 * 7: seven Pubkeys
    /// - 8 * 6: six u64 fields
    /// - 1 + 1: two booleans
    pub const LEN: usize = 8 + 1 + 32 * 7 + 8 * 6 + 1 + 1;
}
