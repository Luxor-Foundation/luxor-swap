use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

/// Admin-only: set (or rotate) the keeper key authorized to run `stake_deposits`.
///
/// Writes to a dedicated `keeper_config` PDA — it does NOT touch `global_config`,
/// so there is no account-layout change and no migration risk to the live
/// program. Rotating the keeper is a single cheap transaction; no program
/// upgrade is required.
#[derive(Accounts)]
pub struct SetKeeper<'info> {
    /// Protocol admin (config admin or hardcoded program admin).
    #[account(
        mut,
        constraint = (owner.key() == global_config.admin || owner.key() == crate::admin::id()) @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// Global protocol configuration (read-only; used for the admin check).
    #[account(
        seeds = [GLOBAL_CONFIG_SEED.as_bytes()],
        bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// The keeper-config account: created on the first call, updated thereafter.
    #[account(
        init_if_needed,
        payer = owner,
        space = KeeperConfig::LEN,
        seeds = [KEEPER_CONFIG_SEED.as_bytes()],
        bump,
    )]
    pub keeper_config: Account<'info, KeeperConfig>,

    pub system_program: Program<'info, System>,
}

/// Store `new_keeper` as the authorized keeper for `stake_deposits`.
pub fn set_keeper(ctx: Context<SetKeeper>, new_keeper: Pubkey) -> Result<()> {
    let kc = &mut ctx.accounts.keeper_config;
    kc.keeper = new_keeper;
    kc.bump = ctx.bumps.keeper_config;
    msg!("Keeper set to {}", new_keeper);
    Ok(())
}
