use anchor_lang::prelude::*;

/// PDA seed for the keeper configuration account.
pub const KEEPER_CONFIG_SEED: &str = "keeper_config";

/// Small, admin-updatable account holding the pubkey authorized to trigger
/// `stake_deposits` (the off-chain keeper).
///
/// It lives in its OWN account — deliberately NOT in `global_config` — so the
/// admin can set or rotate the keeper with a single cheap transaction, with no
/// account-layout change and therefore no risk of breaking the deserialization
/// of any already-deployed account. If this account has never been set,
/// `stake_deposits` falls back to the hardcoded `crate::keeper` default.
#[account]
#[derive(Default, Debug)]
pub struct KeeperConfig {
    /// The key currently authorized to call `stake_deposits`.
    pub keeper: Pubkey,
    /// PDA bump for this account.
    pub bump: u8,
}

impl KeeperConfig {
    /// 8 discriminator + 32 keeper + 1 bump.
    pub const LEN: usize = 8 + 32 + 1;
}
