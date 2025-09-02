use anchor_lang::{prelude::*, solana_program::{program::invoke_signed, sysvar}};
use crate::{error::ErrorCode, states::{GlobalConfig, StakeInfo, UserStakeInfo, ADMIN_STAKE_INFO_SEED, GLOBAL_CONFIG_SEED}, utils::transfer_from_pool_vault_to_user, PRECISION};
use anchor_spl::{associated_token::AssociatedToken, token::spl_token, token_interface::{Mint, TokenAccount, TokenInterface}};
use anchor_lang::solana_program::stake::instruction as stake_ix;

/// Emergency controls for protocol administrators.
///
/// This instruction supports **four** emergency operations, selected by `param`:
/// - `0` → Withdraw **all LXR** from a specified vault (treasury or reward) to admin’s ATA.
/// - `1` → Withdraw **all WSOL** from the SOL treasury vault to admin’s WSOL ATA.
/// - `2` → **Deactivate stake** for the protocol stake PDA (begins cooldown).
/// - `3` → **Withdraw staked SOL** (post-cooldown) from the stake PDA to the admin’s system account.
///
/// Security model:
/// - Only the protocol `admin` or hardcoded program admin may call this (checked on `owner`).
/// - All token movements require the program `authority` PDA to sign via seeds.
/// - Stake CPIs (deactivate/withdraw) also use the `authority` PDA as the stake authority.
#[derive(Accounts)]
pub struct EmergencyWithdraw<'info> {
    /// Admin (must match `global_config.admin` or program admin).
    #[account(
        mut,
        constraint = (owner.key() == global_config.admin || owner.key() == crate::admin::id()) @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// Global protocol configuration.
    #[account(
        seeds = [GLOBAL_CONFIG_SEED.as_bytes()],
        bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Program authority PDA (stake/treasury authority).
    ///
    /// CHECK: PDA derivation enforced by seeds; used only as signer for CPIs.
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// LXR vault to drain (either treasury or reward vault).
    ///
    /// When `param == 0`, this account is the **source** of LXR withdrawn to the admin.
    /// Guarded to ensure it matches **either** `global_config.lxr_treasury_vault` **or**
    /// `global_config.lxr_reward_vault`.
    #[account(
        mut,
        constraint = (luxor_vault_any.key() == global_config.lxr_treasury_vault || luxor_vault_any.key() == global_config.lxr_reward_vault) @ ErrorCode::InvalidVault,
    )]
    pub luxor_vault_any: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(mut,address = global_config.lxr_reward_vault)]
    pub luxor_reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// SOL treasury vault (WSOL). Used when `param == 1`.
    #[account(mut,address = global_config.sol_treasury_vault)]
    pub sol_treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [
            ADMIN_STAKE_INFO_SEED.as_bytes(),
        ],
        bump,
    )]
    pub admin_stake_info: Account<'info, UserStakeInfo>,

    #[account(address = global_config.stake_info)]
    pub stake_info: Account<'info, StakeInfo>,

    /// Canonical LXR mint.
    #[account(address = crate::luxor_mint::id() @ ErrorCode::InvalidLuxorMint)]
    pub luxor_mint: Box<InterfaceAccount<'info, Mint>>,

    /// SPL Native mint (WSOL). Used to create admin WSOL ATA if needed.
    #[account(address = spl_token::native_mint::id() @ ErrorCode::InvalidLuxorMint)]
    pub native_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Admin’s LXR ATA (receiver for param `0`). Created on demand.
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = luxor_mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program, 
    )]
    pub owner_lxr_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Admin’s WSOL ATA (receiver for param `1`). Created on demand.
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = native_mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program, 
    )]
    pub owner_wsol_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Protocol Stake Account (PDA). Target for `deactivate_stake`/`withdraw`.
    ///
    /// CHECK: Address enforced via `global_config.stake_account`.
    #[account(mut,address = global_config.stake_account)]
    pub stake_pda: UncheckedAccount<'info>,

    /// Token program interface (Token-2022).
    pub token_program: Interface<'info, TokenInterface>,

    /// CHECK: Clock sysvar (required by Stake CPIs for slots/epochs).
    #[account(address = sysvar::clock::ID)]
    pub clock: UncheckedAccount<'info>,

    /// Associated Token Program (for ATA creations above).
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// System Program (payer/rent).
    pub system_program: Program<'info, System>,
}

/// Perform one of the emergency operations selected by `param`.
///
/// # Parameters
/// - `param`:
///     - `0` → Withdraw **all LXR** from `luxor_vault_any` → `owner_lxr_token`.
///     - `1` → Withdraw **all WSOL** from `sol_treasury_vault` → `owner_wsol_token`.
///     - `2` → Deactivate stake for `stake_pda` (requires later epoch to withdraw).
///     - `3` → Withdraw `value` lamports from `stake_pda` → `owner` (post-deactivation).
/// - `value`: Used only when `param == 3` (amount to withdraw).
///
/// # Notes
/// - Token withdrawals use `transfer_from_pool_vault_to_user` with `authority` PDA signer seeds.
/// - Stake actions use Stake Program CPIs with `authority` as stake authority signer.
/// - For `param == 3`, make sure the stake is fully or partially deactivated
///   and the requested `value` is available to withdraw.
pub fn emergency_withdraw(ctx: Context<EmergencyWithdraw>, param: u8 , value: u64) -> Result<()> {
    match param {
        0 => {
            // (0) Withdraw all LXR from selected vault (treasury or reward) to admin ATA.
            transfer_from_pool_vault_to_user(
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.luxor_vault_any.to_account_info(),
                ctx.accounts.owner_lxr_token.to_account_info(),
                ctx.accounts.luxor_mint.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
                ctx.accounts.luxor_vault_any.amount,
                ctx.accounts.luxor_mint.decimals,
                &[&[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]]],
            )?;

        }
        1 => {
            // (1) Withdraw all WSOL from SOL treasury vault to admin WSOL ATA.
            transfer_from_pool_vault_to_user(
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.sol_treasury_vault.to_account_info(),
                ctx.accounts.owner_wsol_token.to_account_info(),
                ctx.accounts.native_mint.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
                ctx.accounts.sol_treasury_vault.amount,
                ctx.accounts.native_mint.decimals,
                &[&[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]]],
            )?;
        }
        2 => {
            let admin_stake_info = &mut ctx.accounts.admin_stake_info;
            let stake_info = &ctx.accounts.stake_info;
             
            let reward_per_token_lxr_pending_admin = stake_info.reward_per_token_lxr_stored
            .checked_sub(admin_stake_info.lxr_reward_per_token_completed).unwrap();
    
            let lxr_rewards_to_claim_admin = (admin_stake_info.total_staked_sol as u128)
            .checked_mul(reward_per_token_lxr_pending_admin).unwrap()
            .checked_div(PRECISION).unwrap()
            .checked_div(PRECISION).unwrap() as u64;
    
            admin_stake_info.lxr_rewards_pending = admin_stake_info.lxr_rewards_pending
            .checked_add(lxr_rewards_to_claim_admin).unwrap();
            admin_stake_info.lxr_reward_per_token_completed = stake_info.reward_per_token_lxr_stored;

            transfer_from_pool_vault_to_user(
                ctx.accounts.authority.to_account_info(),
                ctx.accounts.luxor_reward_vault.to_account_info(),
                ctx.accounts.luxor_vault_any.to_account_info(),
                ctx.accounts.luxor_mint.to_account_info(),
                ctx.accounts.token_program.to_account_info(),
                admin_stake_info.lxr_rewards_pending,
                ctx.accounts.luxor_mint.decimals,
                &[&[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]]],
            )?;
            admin_stake_info.lxr_rewards_pending = 0;
        }
        3 => {
            // (2) Deactivate the protocol stake PDA (begin cooldown).
            let auth_bump = ctx.bumps.authority;
            let seeds: &[&[u8]] = &[crate::AUTH_SEED.as_bytes(), &[auth_bump]];
            let ix = stake_ix::deactivate_stake(&ctx.accounts.stake_pda.key(), &ctx.accounts.authority.key());
            let stake_account_ai = ctx.accounts.stake_pda.to_account_info();
            let staker_ai = ctx.accounts.authority.to_account_info();
            let clock_ai = ctx.accounts.clock.to_account_info();
            invoke_signed(&ix, &[stake_account_ai, staker_ai, clock_ai], &[seeds])?;
        }
        4 => {
            // (3) Withdraw lamports from stake PDA to admin system account (post-deactivation).
            let ix = stake_ix::withdraw(
                    &ctx.accounts.stake_pda.key(),
                    &ctx.accounts.authority.key(),
                    &ctx.accounts.owner.key(),
                    value,   // u64, or ALL available
                    None,       // custodian optional
            );
            let auth_bump = ctx.bumps.authority;
            let seeds: &[&[u8]] = &[crate::AUTH_SEED.as_bytes(), &[auth_bump]];
            let stake_account_ai = ctx.accounts.stake_pda.to_account_info();
            let withdrawer_ai = ctx.accounts.authority.to_account_info();
            let destination_ai = ctx.accounts.owner.to_account_info();
            let clock_ai = ctx.accounts.clock.to_account_info();
            invoke_signed(&ix, &[stake_account_ai, withdrawer_ai, destination_ai, clock_ai], &[seeds])?;
        }
        _ => return Err(ErrorCode::InvalidParam.into()),
    }
    Ok(())
}