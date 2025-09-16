use crate::error::ErrorCode;
use crate::states::{ConfigUpdated, GlobalConfig, GLOBAL_CONFIG_SEED};
use anchor_lang::prelude::*;

/// Accounts context for the `update_config` instruction.
///
/// This handler allows only authorized accounts (the current `admin` in `global_config`
/// or the program-level `admin` defined in `crate::admin::id()`) to update specific
/// configuration parameters in the global protocol config.
///
/// # Accounts
/// - `owner`: Must be either the protocol's current admin (stored in `global_config.admin`)
///   or the program's hardcoded admin.
/// - `global_config`: Global configuration account holding protocol-wide parameters.
/// - `system_program`: Standard Solana System Program (included for completeness).
#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    /// Authorized signer: must be the stored admin or the hardcoded program admin.
    #[account(
        constraint = (owner.key() == global_config.admin || owner.key() == crate::admin::id()) @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// Global configuration account to be updated.
    #[account(
        mut,
        seeds = [GLOBAL_CONFIG_SEED.as_bytes()],
        bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// System program (not directly used in updates but required for Anchor context).
    pub system_program: Program<'info, System>,
}

/// Updates selected fields of the global configuration.
///
/// # Parameters
/// - `param`: A selector determining which field to update.
/// - `value`: The new value to assign (interpreted differently depending on `param`).
///
/// # Param Mapping
/// - `0`: **Admin change** → Expects a new admin Pubkey passed via `remaining_accounts[0]`.
/// - `1`: **min_swap_amount** → Sets minimum swap amount (u64).
/// - `2`: **max_swap_amount** → Sets maximum swap amount (u64).
/// - `3`: **fee_treasury_rate** → Updates the treasury fee rate (u64).
/// - `4`: **purchase_enabled** → Toggles purchase (bool, from nonzero value).
/// - `5`: **redeem_enabled** → Toggles redeem (bool, from nonzero value).
///
/// Any other `param` value returns `ErrorCode::InvalidParam`.
///
/// # Errors
/// - `InvalidOwner`: If the caller is not an authorized admin.
/// - `MissingRemainingAccount`: If updating admin but no Pubkey is provided.
/// - `InvalidParam`: If `param` is outside the valid range.
///
/// # Example
/// ```ignore
/// // Change min_swap_amount to 500
/// update_config(ctx, 1, 500)?;
///
/// // Disable purchase
/// update_config(ctx, 4, 0)?;
/// ```
pub fn update_config(ctx: Context<UpdateConfig>, param: u8, value: u64) -> Result<()> {
    let global_config = &mut ctx.accounts.global_config;
    match param {
        // Update admin (requires new admin key from remaining_accounts[0])
        0 => {
            let new_admin = *ctx
                .remaining_accounts
                .iter()
                .next()
                .ok_or(error!(ErrorCode::MissingRemainingAccount))?
                .key;
            require_keys_neq!(new_admin, Pubkey::default());
            global_config.admin = new_admin;
        }
        // Update minimum swap amount
        1 => {
            global_config.min_swap_amount = value;
        }
        // Update maximum swap amount
        2 => {
            global_config.max_swap_amount = value;
        }
        // Update treasury fee rate
        3 => {
            global_config.fee_treasury_rate = value;
        }
        // Toggle purchase_enabled flag
        4 => {
            global_config.purchase_enabled = value != 0;
        }
        // Toggle redeem_enabled flag
        5 => {
            global_config.redeem_enabled = value != 0;
        }
        6 => {
            global_config.max_stake_count_to_get_bonus = value;
        }
        // Invalid parameter selector
        _ => return Err(error!(ErrorCode::InvalidParam)),
    }

    emit!(ConfigUpdated {
        admin: global_config.admin,
        min_swap_amount: global_config.min_swap_amount,
        max_swap_amount: global_config.max_swap_amount,
        fee_treasury_rate: global_config.fee_treasury_rate,
        purchase_enabled: global_config.purchase_enabled,
        redeem_enabled: global_config.redeem_enabled,
    });
    Ok(())
}
