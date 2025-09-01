use anchor_lang::{prelude::*};
use crate::{states::{GlobalConfig, RewardsCollected, StakeInfo, UserStakeInfo, GLOBAL_CONFIG_SEED, USER_STAKE_INFO_SEED}, utils::transfer_from_pool_vault_to_user, PRECISION};
use anchor_spl::{associated_token::AssociatedToken, token_interface::{Mint, TokenAccount, TokenInterface}};
use crate::error::ErrorCode;

/// Redeem pending LXR rewards accrued from staking SOL.
///
/// Reward math overview:
/// - Global index: `stake_info.reward_per_token_lxr_stored` accumulates LXR-per-staked-SOL,
///   scaled by `PRECISION * PRECISION`.
/// - Per-user checkpoint: `user_stake_info.lxr_reward_per_token_completed`
///   stores the index at the user's last claim.
/// - Pending = `(user.total_staked_sol * (global_idx - user_idx)) / PRECISION / PRECISION`.
///
/// Forfeiture (anti-dilution) rule:
/// - If the user's current LXR balance (`owner_lxr_token.amount`) is **below**
///   their recorded base holdings (`user.base_lxr_holdings`), their claimable rewards
///   are **pro-rated** by the ratio `current / base`, and the difference is **forfeited**.
/// - Forfeited rewards are transferred to treasury (`luxor_vault`).
///
/// Funds movement:
/// - Claimable LXR moves from `luxor_reward_vault` → user ATA.
/// - Forfeited LXR (if any) moves from `luxor_reward_vault` → `luxor_vault` (treasury).
#[derive(Accounts)]
pub struct Redeem<'info> {
    /// User claiming rewards (payer for ATA creation if needed).
    #[account(mut)]
    pub owner: Signer<'info>,

    /// Global protocol configuration.
    #[account(
        seeds = [GLOBAL_CONFIG_SEED.as_bytes()],
        bump,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Per-user staking record (derived by USER_STAKE_INFO_SEED + owner).
    #[account(
        seeds = [
            USER_STAKE_INFO_SEED.as_bytes(), 
            owner.key().as_ref()
        ],
        bump,
    )]
    pub user_stake_info: Account<'info, UserStakeInfo>,

    /// Program authority PDA (acts as token authority for vault transfers).
    ///
    /// CHECK: PDA derivation enforced by seeds; used only as a signer.
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// Global staking aggregates and reward indices.
    #[account(
        mut,
        address = global_config.stake_info,
    )]
    pub stake_info: Account<'info, StakeInfo>,

    /// Protocol LXR treasury vault (receives forfeited rewards).
    #[account(mut,address = global_config.lxr_treasury_vault)]
    pub luxor_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// LXR rewards vault (pays out user redemptions).
    #[account(mut,address = global_config.lxr_reward_vault)]
    pub luxor_reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Canonical LXR mint.
    #[account(address = crate::luxor_mint::id() @ ErrorCode::InvalidLuxorMint)]
    pub luxor_mint: Box<InterfaceAccount<'info, Mint>>,

    /// User's LXR ATA; created on demand to receive rewards.
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = luxor_mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program, 
    )]
    pub owner_lxr_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// SPL Token-2022 interface program.
    pub token_program: Interface<'info, TokenInterface>,

    /// Associated Token Program (for ATA init).
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// System Program (for rent/ATA).
    pub system_program: Program<'info, System>,
}

/// Redeems the caller's accrued LXR rewards and applies forfeiture if their
/// current LXR holdings are below recorded `base_lxr_holdings`.
///
/// Steps:
/// 1) Compute pending index delta: `reward_per_token_lxr_pending`.
/// 2) Calculate `lxr_rewards_to_claim` using user's `total_staked_sol`.
/// 3) If user's current LXR < base holdings, pro-rate rewards; track `forfieted_lxr`.
/// 4) Add any `lxr_rewards_pending` already owed to the user.
/// 5) Update user & global tallies and indices.
/// 6) Transfer claimable LXR from rewards vault to user.
/// 7) Transfer forfeited LXR (if any) from rewards vault to treasury.
/// 8) Emit `RewardsCollected`.
pub fn redeem(ctx: Context<Redeem>) -> Result<()> {
    let user_stake_info = &mut ctx.accounts.user_stake_info;
    let stake_info = &mut ctx.accounts.stake_info;

    // --- 1) Pending index delta (must be positive) ---
    let reward_per_token_lxr_pending = stake_info.reward_per_token_lxr_stored
        .checked_sub(user_stake_info.lxr_reward_per_token_completed)
        .unwrap();
    require!(reward_per_token_lxr_pending > 0, ErrorCode::NoRewardsToClaim);

    // --- 2) Base rewards = stake * delta_index, scaled down by PRECISION^2 ---
    let mut lxr_rewards_to_claim = (user_stake_info.total_staked_sol as u128)
        .checked_mul(reward_per_token_lxr_pending).unwrap()
        .checked_div(PRECISION).unwrap()
        .checked_div(PRECISION).unwrap() as u64;
    require!(lxr_rewards_to_claim > 0, ErrorCode::NoRewardsToClaim);

    // --- 3) Forfeiture if current holdings < base holdings ---
    let mut forfieted_lxr = 0;
    if ctx.accounts.owner_lxr_token.amount < user_stake_info.base_lxr_holdings {
        let lxr_holdings = ctx.accounts.owner_lxr_token.amount;
        let full_rewards = lxr_rewards_to_claim;

        // Pro-rate rewards by current/base ratio
        lxr_rewards_to_claim = (lxr_holdings as u128)
            .checked_mul(lxr_rewards_to_claim as u128).unwrap()
            .checked_div(user_stake_info.base_lxr_holdings as u128).unwrap() as u64;

        forfieted_lxr = full_rewards.checked_sub(lxr_rewards_to_claim).unwrap(); 
    }

    // --- 4) Include any pending carryover ---
    lxr_rewards_to_claim = lxr_rewards_to_claim.checked_add(user_stake_info.lxr_rewards_pending).unwrap();

    // --- 5) Update user & global tallies and indices ---
    // User updates
    user_stake_info.total_lxr_claimed = user_stake_info.total_lxr_claimed.checked_add(lxr_rewards_to_claim).unwrap();
    user_stake_info.total_lxr_forfeited = user_stake_info.total_lxr_forfeited.checked_add(forfieted_lxr).unwrap();
    user_stake_info.lxr_reward_per_token_completed = stake_info.reward_per_token_lxr_stored;
    user_stake_info.lxr_rewards_pending = 0;

    // Global updates
    stake_info.total_lxr_claimed = stake_info.total_lxr_claimed.checked_add(lxr_rewards_to_claim).unwrap();
    stake_info.total_lxr_forfeited = stake_info.total_lxr_forfeited.checked_add(forfieted_lxr).unwrap();

    // --- 6) Pay claimable rewards from reward vault → user ---
    transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.luxor_reward_vault.to_account_info(),
        ctx.accounts.owner_lxr_token.to_account_info(),
        ctx.accounts.luxor_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        lxr_rewards_to_claim,
        ctx.accounts.luxor_mint.decimals,
        &[&[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]]],
    )?;

    // --- 7) Send forfeited rewards (if any) from reward vault → treasury ---
    if forfieted_lxr > 0 {
        transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.luxor_reward_vault.to_account_info(),
        ctx.accounts.luxor_vault.to_account_info(),
        ctx.accounts.luxor_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        forfieted_lxr,
        ctx.accounts.luxor_mint.decimals,
        &[&[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]]],
        )?;
    }

    // --- 8) Event for indexers/UX ---
    emit!(RewardsCollected{
        collector: ctx.accounts.owner.key(),
        lxr_collected: lxr_rewards_to_claim,
        lxr_forfeited: forfieted_lxr,
    });

    Ok(())
}
