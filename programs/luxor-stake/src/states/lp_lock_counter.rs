use anchor_lang::prelude::*;

pub const LP_LOCK_COUNTER_SEED: &str = "lp_lock_counter";

/// Tracks the number and total amount of LP token locks created by a user for a specific LP mint.
/// This account is uniquely identified by the combination of `(user, lp_mint)`.
#[account]
#[derive(Default, Debug)]
pub struct LpLockCounter {
    pub user: Pubkey,           // Wallet address of the user
    pub lp_mint: Pubkey,        // Mint address of the LP token being tracked
    pub total_lock_count: u64, // Total number of lock positions created by this user for the given LP
    pub total_lock_amount: u64, // Cumulative LP tokens locked by this user for the given LP
}

impl LpLockCounter {
    /// Total space required for the LpLockCounter account (in bytes)
    pub const LEN: usize = 8 +   // discriminator
        32 +  // user
        32 +  // lp_mint
        8 +   // total_lock_count
        8; // total_lock_amount
}
