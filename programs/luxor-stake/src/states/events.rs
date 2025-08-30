use anchor_lang::prelude::*;

#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct LpLockEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub lp_mint: Pubkey,
    pub locked_perm: bool,
}

#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct LpUnlockEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub lp_mint: Pubkey,
}

#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct CollectFeesEvent {
    pub user: Pubkey,
    pub lp_mint: Pubkey,
    pub token_0_amount: u64,
    pub token_1_amount: u64,
}
