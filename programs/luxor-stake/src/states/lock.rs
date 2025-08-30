use anchor_lang::prelude::*;

pub const USER_LOCK_SEED: &str = "user_lock";

/// Stores information about a specific LP token lock created by a user.
/// Each lock is uniquely identified by `(user, lp_mint, lock_count)`.
#[account]
#[derive(Default, Debug)]
pub struct UserLock {
    pub bump: u8,                    // PDA bump for address derivation
    pub user: Pubkey,                // Wallet that owns this lock
    pub lp_mint: Pubkey,             // Mint address of the LP token being locked
    pub lock_count: u64, // Counter/index to distinguish multiple locks for same user + LP mint
    pub lock_amount: u64, // Amount of LP tokens locked
    pub unlock_time: u64, // Unix timestamp after which LPs can be unlocked (0 if permanent)
    pub principal_token_0: u64, // Underlying token 0 amount at time of lock
    pub principal_token_1: u64, // Underlying token 1 amount at time of lock
    pub principal_liquidity: u64, // Liquidity value of LP tokens at lock time
    pub is_locked_permanently: bool, // True if lock is permanent and unlock is disabled
    pub token_0_fees_collected: u64, // Accumulated token 0 fees (for reporting/UI)
    pub token_1_fees_collected: u64, // Accumulated token 1 fees (for reporting/UI)
    pub is_unlocked: bool, // Flag indicating whether this lock has already been unlocked
    pub last_updated: u64, // Last update timestamp (useful for syncing/indexing)
    pub created_at: u64, // Timestamp when the lock was created
}

impl UserLock {
    /// Total space required for the UserLock account (in bytes)
    pub const LEN: usize = 8 +   // discriminator
        1 +   // bump
        32 +  // user
        32 +  // lp_mint
        8 +   // lock_count
        8 +   // lock_amount
        8 +   // unlock_time
        8 +   // principal_token_0
        8 +   // principal_token_1
        8 +   // principal_liquidity
        1 +   // is_locked_permanently
        8 +   // token_0_fees_collected
        8 +   // token_1_fees_collected
        1 +   // is_unlocked
        8 +   // last_updated
        8; // created_at
}
