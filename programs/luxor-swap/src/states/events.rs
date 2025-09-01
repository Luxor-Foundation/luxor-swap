use anchor_lang::prelude::*;

//
// ──────────────────────────────────────────────────────────────────────────────
// Events: Emitted for off-chain indexers/clients to track protocol state changes
// ──────────────────────────────────────────────────────────────────────────────
//

/// Emitted once when the global configuration is initialized.
///
/// Captures all critical addresses and tunable parameters at genesis so
/// indexers/frontends can cache protocol settings without re-reading accounts.
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct GlobalConfigInitialized {
    /// Protocol admin pubkey (may later be updated).
    pub admin: Pubkey,
    /// Canonical LUXOR mint.
    pub luxor_mint: Pubkey,
    /// Treasury LXR vault (protocol-owned).
    pub lxr_treasury_vault: Pubkey,
    /// Rewards LXR vault (pays user redemptions).
    pub lxr_reward_vault: Pubkey,
    /// Stake account PDA (owned by Stake program).
    pub stake_account: Pubkey,
    /// Chosen validator vote account for delegation.
    pub vote_account: Pubkey,
    /// Aggregate staking state account.
    pub stake_info: Pubkey,
    /// Early-bird bonus rate applied to purchases (denominated against a common fee denominator).
    pub bonus_rate: u64,
    /// Maximum stake count threshold where bonus still applies.
    pub max_stake_count_to_get_bonus: u64,
    /// Minimum LXR allowed per swap/purchase.
    pub min_swap_amount: u64,
    /// Maximum LXR allowed per swap/purchase.
    pub max_swap_amount: u64,
    /// Treasury fee rate applied to certain flows (e.g., buybacks).
    pub fee_treasury_rate: u64,
    /// Global switch to enable purchasing.
    pub purchase_enabled: bool,
    /// Global switch to enable redemption.
    pub redeem_enabled: bool,
    /// Initial LXR allocation reference used in pricing/scaling logic.
    pub initial_lxr_allocation_vault: u64,
}

/// Emitted whenever configuration parameters are modified via `update_config`.
///
/// Useful for tracking governance/admin actions that change economic parameters.
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct ConfigUpdated {
    /// Current admin (may be the same or newly set).
    pub admin: Pubkey,
    /// New minimum swap/purchase amount.
    pub min_swap_amount: u64,
    /// New maximum swap/purchase amount.
    pub max_swap_amount: u64,
    /// Updated treasury fee rate.
    pub fee_treasury_rate: u64,
    /// Whether purchasing is enabled after the update.
    pub purchase_enabled: bool,
    /// Whether redemption is enabled after the update.
    pub redeem_enabled: bool,
}

/// Emitted when a user buys LXR through the regular purchase path.
///
/// Encodes the exact SOL paid and LXR received for auditing/analytics.
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct LxrPurchased {
    /// The user who performed the purchase.
    pub purchaser: Pubkey,
    /// SOL paid (in lamports).
    pub sol_amount: u64,
    /// LXR received (base units).
    pub lxr_amount: u64,
}

/// Emitted when an admin records a manual purchase on behalf of a user.
///
/// Used for backfills/adjustments where pricing was handled externally.
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct ManualLxrPurchased {
    /// Beneficiary user for whom the manual purchase is recorded.
    pub purchaser: Pubkey,
    /// SOL counted as spent (in lamports).
    pub sol_amount: u64,
    /// LXR credited (base units).
    pub lxr_amount: u64,
}

/// Emitted after executing a buyback using accrued SOL stake rewards.
///
/// Shows SOL consumed, LXR acquired, and protocol fee routed to treasury.
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct BuybackExecuted {
    /// Total SOL rewards used for this buyback (before fee), in lamports.
    pub sol_amount: u64,
    /// LXR acquired from the market (base units).
    pub lxr_bought: u64,
    /// Fee portion of SOL sent to treasury (in lamports).
    pub fee_to_treasury: u64,
}

/// Emitted when a user redeems their LXR rewards.
///
/// Includes both the amount collected and any forfeiture applied due to
/// holdings falling below recorded base holdings.
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct RewardsCollected {
    /// The user who claimed rewards.
    pub collector: Pubkey,
    /// LXR paid out to the user (base units).
    pub lxr_collected: u64,
    /// LXR forfeited to treasury due to shortfall vs base holdings (base units).
    pub lxr_forfeited: u64,
}
