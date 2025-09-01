use crate::curve::{CurveCalculator, FEE_RATE_DENOMINATOR_VALUE};
use crate::error::ErrorCode;
use crate::utils::transfer_from_pool_vault_to_user;
use crate::{states::*, PRECISION};
use anchor_lang::{prelude::*, solana_program};
use anchor_lang::solana_program::program::{invoke, invoke_signed};
use anchor_lang::solana_program::stake::instruction as stake_ix;
use anchor_lang::solana_program::system_instruction::transfer;
use anchor_lang::solana_program::{stake, sysvar};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

/// Accounts required for purchasing LXR with SOL and delegating stake.
///
/// Flow summary:
/// 1) Validate pool state & vaults, compute SOL required for target `lxr_to_purchase`.
/// 2) Apply bonus pricing until `max_stake_count_to_get_bonus`; after that, scale price
///    by treasury inventory vs initial allocation.
/// 3) Realize any newly accrued SOL rewards on the stake PDA and update `stake_info`.
/// 4) Transfer SOL from user → stake PDA, then delegate the stake to a `vote_account`
///    using the program authority PDA.
/// 5) Mint/transfer LXR from vault to user ATA and update per-user aggregates.
/// 6) Emit `LxrPurchased` event.
#[derive(Accounts)]
pub struct Purchase<'info> {
    /// User paying SOL and receiving LXR.
    #[account(mut)]
    pub owner: Signer<'info>,

    /// Global configuration; purchase must be enabled.
    #[account(
        seeds = [GLOBAL_CONFIG_SEED.as_bytes()],
        bump,
        constraint = global_config.purchase_enabled @ ErrorCode::PurchaseDisabled,
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Program treasury vault that holds LUXOR to be sold to users.
    #[account(
        mut,
        address = global_config.lxr_treasury_vault,
    )]
    pub luxor_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Per-user staking metadata (created lazily).
    #[account(
        init_if_needed,
        seeds = [
            USER_STAKE_INFO_SEED.as_bytes(), 
            owner.key().as_ref()
        ],
        bump,
        payer = owner,
        space = UserStakeInfo::LEN
    )]
    pub user_stake_info: Account<'info, UserStakeInfo>,

    /// Global stake meta (totals and reward indices).
    #[account(
        mut,
        address = global_config.stake_info,
    )]
    pub stake_info: Account<'info, StakeInfo>,

    /// Program authority PDA used for stake delegation.
    ///
    /// CHECK: PDA derivation is enforced by seeds; used as a signing PDA.
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// Canonical LUXOR mint.
    #[account(address = crate::luxor_mint::id())]
    pub luxor_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Payer's LXR ATA; created if missing so they can receive purchased LXR.
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = luxor_mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program,  
    )]
    pub owner_lxr_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Stake account (PDA) that receives SOL and is delegated to `vote_account`.
    ///
    /// CHECK: Address comes from `global_config.stake_account`; owned by Stake program.
    #[account(
        mut,
        address = global_config.stake_account
    )]
    pub stake_pda: UncheckedAccount<'info>,

    /// Target validator vote account to which stake is delegated.
    ///
    /// CHECK: Externally provided, validated by CPI to Stake program.
    #[account(address = global_config.vote_account)]
    pub vote_account: UncheckedAccount<'info>,

    /// Stake program (CPI target).
    ///
    /// CHECK: Program ID only.
    #[account(address = stake::program::ID)]
    pub stake_program: UncheckedAccount<'info>,

    /// Clock sysvar required by `delegate_stake`.
    ///
    /// CHECK: Program ID only.
    #[account(address = sysvar::clock::ID)]
    pub clock: UncheckedAccount<'info>,

    /// Stake history sysvar required by `delegate_stake`.
    ///
    /// CHECK: Program ID only.
    #[account(address = sysvar::stake_history::ID)]
    pub stake_history: UncheckedAccount<'info>,

    /// Stake config account required by `delegate_stake` (fixed program address).
    ///
    /// CHECK: Program ID only.
    #[account(address = solana_program::stake::config::ID)]
    pub stake_config: UncheckedAccount<'info>,

    /// Raydium pool state used to compute swap price for LXR in SOL terms.
    ///
    /// CHECK: Address pinned via `luxor_pool_state::id()`.
    #[account(
        address = crate::luxor_pool_state::id()
    )]
    pub pool_state: UncheckedAccount<'info>,

    /// Pool vault for token_0 (pricing input).
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Pool vault for token_1 (pricing output).
    pub token_1_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// SPL Token-2022 interface program.
    pub token_program: Interface<'info, TokenInterface>,

    /// Associated Token Program (for ATA creation).
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// System Program (for SOL transfers).
    pub system_program: Program<'info, System>,
}

/// Purchase LXR with SOL and immediately delegate the deposited SOL as stake.
///
/// # Parameters
/// - `lxr_to_purchase`: Exact LXR amount desired by the user (base units).
/// - `max_sol_amount`: Max SOL the user is willing to pay for the purchase (slippage cap).
///
/// # Pricing / Mechanics
/// - Uses pool state (`pool_state`) to compute the required SOL input for the exact LXR output
///   via `CurveCalculator::swap_base_output(...)`.
/// - Applies an early-bird bonus discount to the SOL needed if `total_stake_count + 1` is within
///   `max_stake_count_to_get_bonus`; otherwise scales price with treasury inventory.
/// - Ensures constant product is non-decreasing and the exact output matches `lxr_to_purchase`.
///
/// # Rewards Accrual
/// - If the stake PDA balance has increased since last observation, considers it accrued rewards:
///   updates `total_sol_rewards_accrued`, `last_tracked_sol_balance`, and
///   increases `reward_per_token_sol_stored` based on `PRECISION / total_staked_sol`.
///
/// # Side Effects
/// - Transfers `total_sol_needed` SOL from user to stake PDA, delegates to `vote_account`.
/// - Sends `lxr_to_purchase` LXR from treasury vault to the user's ATA.
/// - Updates global and per-user staking aggregates; emits `LxrPurchased`.
///
/// # Fails
/// - `PurchaseDisabled` if purchases are globally disabled.
/// - `ZeroTradingTokens` or arithmetic errors if pricing fails.
/// - `require_*` guards for invariants, slippage (`max_sol_amount`), and pool addresses.
pub fn purchase(ctx: Context<Purchase>, lxr_to_purchase: u64, max_sol_amount: u64) -> Result<()> {
    require_gt!(lxr_to_purchase, 0);
    
    // --- Load and validate pool state/vaults used for pricing ---
    let pool_state_info = &ctx.accounts.pool_state;
    let pool_state = PoolState::try_deserialize(&mut &pool_state_info.data.borrow()[..])?;
    require_keys_eq!(pool_state.token_0_vault, ctx.accounts.token_0_vault.key());
    require_keys_eq!(pool_state.token_1_vault, ctx.accounts.token_1_vault.key());

    // Exact tokens user wants to receive (post any transfer fee logic, if applicable).
    let amount_out_with_transfer_fee = lxr_to_purchase;

    // Compute swap parameters from pool state/current vault balances.
    let SwapParams {
        trade_direction : _,
        total_input_token_amount,
        total_output_token_amount,
        token_0_price_x64: _,
        token_1_price_x64: _,
        is_creator_fee_on_input,
    } = pool_state.get_swap_params(
        ctx.accounts.token_0_vault.key(),
        ctx.accounts.token_1_vault.key(),
        ctx.accounts.token_0_vault.amount,
        ctx.accounts.token_1_vault.amount,
    )?;

    // Constant-product before swap (sanity/invariant check).
    let constant_before = u128::from(total_input_token_amount)
        .checked_mul(u128::from(total_output_token_amount))
        .unwrap();

    // Compute creator fee rate (example uses 500 as a baseline).
    let creator_fee_rate =
        pool_state.adjust_creator_fee_rate(500);

    // Price the exact-output trade (how much SOL is needed).
    let result = CurveCalculator::swap_base_output(
        u128::from(amount_out_with_transfer_fee),
        u128::from(total_input_token_amount),
        u128::from(total_output_token_amount),
        2500,          // trade fee
        creator_fee_rate,
        120000,        // protocol fee
        40000,         // fund fee
        is_creator_fee_on_input,
    )
    .ok_or(ErrorCode::ZeroTradingTokens)?;

    // Constant-product after swap must be ≥ before (no reversal of invariant).
    let constant_after = u128::from(result.new_input_vault_amount)
        .checked_mul(u128::from(result.new_output_vault_amount))
        .unwrap();
    
    // Must receive exactly what was requested.
    require_eq!(
        u64::try_from(result.output_amount).unwrap(),
        amount_out_with_transfer_fee
    );
    
    require_gte!(constant_after, constant_before);

    // Raw SOL needed from pricing path.
    let mut total_sol_needed = u64::try_from(result.input_amount).unwrap();
    
    let stake_info = &mut ctx.accounts.stake_info;
    let user_stake_info = &mut ctx.accounts.user_stake_info;
    let global_config = &ctx.accounts.global_config;

    // --- Bonus / post-bonus pricing adjustments ---
    if stake_info.total_stake_count + 1  <= global_config.max_stake_count_to_get_bonus {
       total_sol_needed = total_sol_needed
        .checked_sub(
            total_sol_needed
        .checked_mul(global_config.bonus_rate).unwrap()
        .checked_div(FEE_RATE_DENOMINATOR_VALUE).unwrap()
       ).unwrap();
    } else {
        // After bonus phase, scale price against inventory depth.
        total_sol_needed = u128::from(total_sol_needed)
        .checked_mul(ctx.accounts.luxor_vault.amount as u128).unwrap()
        .checked_div(global_config.initial_lxr_allocation_vault as u128).unwrap() as u64;
    }

    // Slippage/limit check from the payer.
    require_gte!(max_sol_amount, total_sol_needed);

    // --- Realize newly accrued SOL rewards on stake PDA (if any) ---
    if ctx.accounts.stake_pda.lamports() > stake_info.last_tracked_sol_balance {
        let rewards_accured = ctx.accounts.stake_pda.lamports()
            .checked_sub(stake_info.last_tracked_sol_balance).unwrap();
        stake_info.total_sol_rewards_accrued = stake_info.total_sol_rewards_accrued
            .checked_add(rewards_accured).unwrap();
        stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
        stake_info.reward_per_token_sol_stored = stake_info.reward_per_token_sol_stored.checked_add(
            (rewards_accured as u128)
            .checked_mul(PRECISION).unwrap()
            .checked_div(stake_info.total_staked_sol as u128).unwrap()
        ).unwrap();
    }

    // --- Transfer SOL from user to stake PDA (fund stake) ---
    let ix = transfer(&ctx.accounts.owner.key(), &ctx.accounts.stake_pda.key(), total_sol_needed);
    invoke(
    &ix,
    &[
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.stake_pda.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
    ],)?;

    // --- Delegate stake to the configured validator ---
    let stake_key = ctx.accounts.stake_pda.key();
    let vote_key  = ctx.accounts.vote_account.key();
    let auth_key  = ctx.accounts.authority.key();

    // Build CPI ix to Stake program: delegate stake.
    let ix = stake_ix::delegate_stake(&stake_key, &auth_key, &vote_key);

    let account_infos = &[
        ctx.accounts.stake_pda.to_account_info(),
        ctx.accounts.vote_account.to_account_info(),
        ctx.accounts.clock.to_account_info(),
        ctx.accounts.stake_history.to_account_info(),
        ctx.accounts.stake_config.to_account_info(),
        ctx.accounts.authority.to_account_info(),
    ];

    // PDA seeds for authority (PDA acts as signer).
    let auth_bump = ctx.bumps.authority;
    let seeds: &[&[u8]] = &[crate::AUTH_SEED.as_bytes(), &[auth_bump]];

    invoke_signed(&ix, account_infos, &[seeds])?;

    // --- Global stake info updates ---
    stake_info.total_staked_sol = stake_info.total_staked_sol
        .checked_add(total_sol_needed).unwrap();
    stake_info.total_stake_count = stake_info.total_stake_count
        .checked_add(1).unwrap();
    stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;
    stake_info.last_update_timestamp = block_timestamp;

    // --- User stake info updates (lazy init + aggregates) ---
    if user_stake_info.owner == Pubkey::default() {
        user_stake_info.owner = ctx.accounts.owner.key();
        user_stake_info.bump = ctx.bumps.user_stake_info;
        user_stake_info.lxr_reward_per_token_completed = stake_info.reward_per_token_lxr_stored;
    }
    user_stake_info.total_staked_sol = user_stake_info.total_staked_sol
        .checked_add(total_sol_needed).unwrap();
    user_stake_info.base_lxr_holdings = user_stake_info.base_lxr_holdings
        .checked_add(lxr_to_purchase).unwrap();

    // --- Transfer purchased LXR from treasury vault to user ATA ---
    transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.luxor_vault.to_account_info(),
        ctx.accounts.owner_lxr_token.to_account_info(),
        ctx.accounts.luxor_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        lxr_to_purchase,
        ctx.accounts.luxor_mint.decimals,
        &[&[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]]],
    )?;

    // --- Emit event for off-chain consumers/indexers ---
    emit!(LxrPurchased {
        purchaser: ctx.accounts.owner.key(),
        sol_amount: total_sol_needed,
        lxr_amount: lxr_to_purchase,
    });

    Ok(())
}