use crate::curve::CurveCalculator;
use crate::curve::FEE_RATE_DENOMINATOR_VALUE;
use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::transfer_from_user_to_pool_vault;
use crate::AUTH_SEED;
use crate::PRECISION;
use crate::STAKE_ACCOUNT_SEED;
use crate::STAKE_SPLIT_ACCOUNT_SEED;
use anchor_lang::prelude::borsh::BorshDeserialize;
use anchor_lang::prelude::borsh::BorshSerialize;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_lang::solana_program::instruction::Instruction;
use anchor_lang::solana_program::program::invoke;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::stake;
use anchor_lang::solana_program::stake::state::StakeStateV2;
use anchor_lang::solana_program::system_instruction;
use anchor_lang::solana_program::system_instruction::transfer;
use anchor_lang::solana_program::sysvar;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::spl_token;
use anchor_spl::token::spl_token::instruction::sync_native;
use anchor_spl::token::Token;
use anchor_spl::token_interface::{Mint, TokenAccount};
use anchor_lang::solana_program::stake::instruction as stake_ix;
use std::mem::size_of;

/// Anchor-encoded parameters for Raydium's `swap_base_input` CPI call.
/// Represents an exact-input trade where `amount_in` is spent to receive
/// at least `minimum_amount_out` of the output token.
#[derive(BorshSerialize, BorshDeserialize)]
pub struct SwapBaseInput {
    /// Exact amount of input tokens to spend.
    amount_in: u64,
    /// Minimum acceptable output (slippage guard).
    minimum_amount_out: u64,
}

/// Accounts required to perform protocol **buyback** using SOL rewards accrued
/// in the stake PDA. The flow:
///
/// 1. Accrue any newly observed SOL rewards on the stake PDA into `stake_info`.
/// 2. Compute rewards available for buyback: `total_sol_rewards_accrued - total_sol_used_for_buyback`.
/// 3. Transfer that SOL (WSOL via native account) to a temporary token account (`token_0_account`)
///    owned by the admin, then `sync_native`.
/// 4. Deduct a treasury fee (`fee_treasury_rate`) from the available SOL to get `actual_amount_in`.
/// 5. Price an **exact-input** swap via `CurveCalculator::swap_base_input` and sanity-check invariants.
/// 6. Execute Raydium CPMM `swap_base_input` CPI to buy LXR.
/// 7. Send acquired LXR to `luxor_reward_vault` and the fee (in SOL/WSOL) to `sol_treasury_vault`.
/// 8. Update reward indices and emit `BuybackExecuted`.
#[derive(Accounts)]
pub struct Buyback<'info> {
    /// Admin signer (must be current protocol admin or hardcoded program admin).
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

    /// Global staking state and reward indices.
    #[account(
        mut,
        address = global_config.stake_info,
    )]
    pub stake_info: Account<'info, StakeInfo>,

    /// CHECK: Vote account to delegate stake to.
    #[account(address = global_config.vote_account)]
    pub vote_account: UncheckedAccount<'info>,

    /// PDA stake account holding staked SOL and accruing rewards.
    ///
    /// CHECK: PDA seeds ensure derivation; expected to be owned by Stake program.
    #[account(
        mut,
        seeds = [STAKE_ACCOUNT_SEED.as_bytes()],
        bump
    )]
    pub stake_pda: UncheckedAccount<'info>,

    /// CHECK: PDA seeds ensure derivation; expected to be owned by Stake program.
    #[account(
        mut,
        seeds =
        [
            STAKE_SPLIT_ACCOUNT_SEED.as_bytes(),
            &stake_info.buyback_count.to_le_bytes()
        ],
        bump
    )]
    pub stake_split_pda: UncheckedAccount<'info>,

    /// CHECK: authority
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,


    /// Vault for accumulated LXR rewards (destination for bought LXR).
    #[account(mut,address = global_config.lxr_reward_vault)]
    pub luxor_reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Treasury vault to receive protocol fee (in SOL/WSOL terms).
    #[account(mut,address = global_config.sol_treasury_vault)]
    pub sol_treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Admin's temporary token account to receive **input token** (token_0, typically WSOL).
    /// Created if missing; later used as the input account for the Raydium swap.
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = vault_0_mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program,
    )]
    pub token_0_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Admin's temporary token account to receive **output token** (token_1, expected to be LXR).
    #[account(
        init_if_needed,
        payer = owner,
        associated_token::mint = vault_1_mint,
        associated_token::authority = owner,
        associated_token::token_program = token_program,
    )]
    pub token_1_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Raydium pool input token vault (token_0 vault, mutable due to swap).
    #[account(mut)]
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Raydium pool output token vault (token_1 vault, mutable due to swap).
    #[account(mut)]
    pub token_1_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Mint for token_0 vault (must match).
    #[account(address = token_0_vault.mint)]
    pub vault_0_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Mint for token_1 vault (must match).
    #[account(address = token_1_vault.mint)]
    pub vault_1_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Raydium pool state (pricing & parameters source).
    ///
    /// CHECK: Address pinned in code; deserialized ad-hoc.
    #[account(mut,address = crate::luxor_pool_state::id())]
    pub pool_state: UncheckedAccount<'info>,

    /// Raydium vault / LP mint authority PDA for the pool (fixed).
    ///
    /// CHECK: Program address checked by constant; used as read-only meta.
    #[account(address = crate::vault_and_lp_mint_auth::id())]
    pub raydium_authority: UncheckedAccount<'info>,

    /// Raydium AMM config account (fee/parameters).
    ///
    /// CHECK: Passed through to Raydium CPI.
    pub amm_config: UncheckedAccount<'info>,

    /// Raydium observation state (TWAP / oracle buffers, etc.).
    ///
    /// CHECK: Passed through to Raydium CPI.
    #[account(mut)]
    pub observation_state: UncheckedAccount<'info>,

    /// CHECK: Raydium CPMM program ID (CPI target).
    #[account(mut,address = crate::raydium_cpmm::id())]
    pub raydium_cpmm_program: AccountInfo<'info>,

    /// CHECK: Stake program ID (CPI target).
    #[account(address = stake::program::ID)]
    pub stake_program: UncheckedAccount<'info>,

    /// CHECK: Clock sysvar (CPI target).
    #[account(address = sysvar::clock::ID)]
    pub clock: UncheckedAccount<'info>,

    /// CHECK: Stake history sysvar (CPI target).
    #[account(address = sysvar::stake_history::ID)]
    pub stake_history: UncheckedAccount<'info>,

    /// CHECK: Stake config sysvar (CPI target).
    #[account(address = solana_program::stake::config::ID)]
    pub stake_config: UncheckedAccount<'info>,

    /// SPL Token program (used both for WSOL sync and token transfers).
    pub token_program: Program<'info, Token>,

    /// Associated Token Program (for creating ATAs as needed).
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// System Program (for SOL transfers from stake PDA).
    pub system_program: Program<'info, System>,
}

/// Executes the protocol buyback of LXR using accrued **SOL staking rewards**.
///
/// The rewards must be un-staked before they can be spent, which on Solana is a
/// multi-transaction, multi-epoch process. To avoid combining incompatible stake
/// operations in a single transaction (which is what made the previous version
/// fail simulation), this instruction is a small state machine that advances
/// **one step per call**, driven by the on-chain state of the per-buyback split
/// stake account (`stake_split_pda`):
///
/// - **Step 1 – Split** (split account still System-owned == not created yet):
///   peel only the accrued rewards off the *active* main stake into a fresh
///   split stake account. The staked principal is never touched or re-delegated.
/// - **Step 2 – Deactivate** (split account active): deactivate the split
///   account. This is a *separate transaction* from the split — that combination
///   is exactly what previously failed.
/// - **Step 3 – Execute** (split account fully cooled down): withdraw the split,
///   swap SOL -> LXR on Raydium, route LXR to `luxor_reward_vault` and the
///   treasury fee to `sol_treasury_vault`, update the reward index, and finish.
///
/// Between Step 2 and Step 3 the caller must wait ~1 epoch for the split to cool
/// down; calling Step 3 early returns `CooldownInProgress`.
pub fn buyback(ctx: Context<Buyback>) -> Result<()> {
    // ─────────────────────────────────────────────────────────────────────
    // BUYBACK FIX — WHAT CHANGED HERE AND WHY
    //
    // BEFORE: the old "request" path deactivated the ENTIRE main stake AND
    //   split it in the SAME transaction, then re-delegated the whole stake on
    //   the next call. Solana rejects a deactivate + split on the same stake
    //   account within one transaction, so the buyback failed at simulation.
    //
    // AFTER: this function is now a 3-step state machine (see the STEP blocks
    //   below). It (1) splits off ONLY the accrued rewards from the still-active
    //   main stake, (2) deactivates that split in a SEPARATE transaction — the
    //   actual fix — and (3) withdraws + swaps after the cooldown epoch. The
    //   main stake / user principal is never deactivated or re-delegated.
    //
    // WHY the step is chosen from the split account's on-chain state (its owner
    //   + deactivation_epoch) instead of a new StakeInfo field: that keeps the
    //   account layout unchanged, so no migration is needed on the already-
    //   deployed program.
    // ─────────────────────────────────────────────────────────────────────
    let stake_info = &mut ctx.accounts.stake_info;
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;
    let space = size_of::<StakeStateV2>();
    let min_rent = Rent::get()?.minimum_balance(space);
    require!(min_rent > 0, ErrorCode::InsufficientRent);

    // --- Accrue any newly observed SOL rewards on the main stake PDA ---
    if ctx.accounts.stake_pda.lamports() > stake_info.last_tracked_sol_balance {
        let rewards_accured = ctx
            .accounts
            .stake_pda
            .lamports()
            .checked_sub(stake_info.last_tracked_sol_balance)
            .unwrap();
        stake_info.total_sol_rewards_accrued = stake_info
            .total_sol_rewards_accrued
            .checked_add(rewards_accured)
            .unwrap();
        stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
    }

    // PDA signer seeds for the program authority (stake authority for all CPIs).
    let auth_bump = ctx.bumps.authority;
    let auth_seeds: &[&[u8]] = &[AUTH_SEED.as_bytes(), &[auth_bump]];

    let authority_ai = ctx.accounts.authority.to_account_info();
    let clock_ai = ctx.accounts.clock.to_account_info();
    let stake_history_ai = ctx.accounts.stake_history.to_account_info();
    let stake_config_ai = ctx.accounts.stake_config.to_account_info();
    let split_ai = ctx.accounts.stake_split_pda.to_account_info();

    let split_owner = *ctx.accounts.stake_split_pda.owner;

    // =====================================================================
    // STEP 1 — SPLIT (split account still System-owned => not created yet).
    // Peel ONLY the accrued-rewards portion off the *active* main stake into
    // a fresh split stake account. The staked principal is left untouched.
    // =====================================================================
    if split_owner == ctx.accounts.system_program.key() {
        require!(!stake_info.buyback_requested, ErrorCode::BuybackAlreadyRequested);

        let reward_available_to_buyback = stake_info
            .total_sol_rewards_accrued
            .checked_sub(stake_info.total_sol_used_for_buyback)
            .unwrap();
        require_gt!(reward_available_to_buyback, 0);
        msg!("Buyback step 1: splitting rewards = {}", reward_available_to_buyback);

        // 1a) Create the split stake account (owned by the Stake program).
        let split_bump = ctx.bumps.stake_split_pda;
        let split_seeds: &[&[u8]] = &[
            STAKE_SPLIT_ACCOUNT_SEED.as_bytes(),
            &stake_info.buyback_count.to_le_bytes(),
            &[split_bump],
        ];
        let create_ix = system_instruction::create_account(
            &ctx.accounts.owner.key(),
            &ctx.accounts.stake_split_pda.key(),
            min_rent,
            space as u64,
            &stake::program::ID,
        );
        invoke_signed(
            &create_ix,
            &[
                ctx.accounts.owner.to_account_info(),
                split_ai.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[split_seeds],
        )?;

        // 1b) Split the rewards off the *active* main stake. Splitting an active
        //     stake is allowed and the destination inherits the active delegation.
        //     We deliberately do NOT deactivate here — deactivation happens in a
        //     later, separate transaction (Step 2). Doing both in one transaction
        //     is exactly what made the old buyback fail simulation.
        let split_ix = &stake_ix::split(
            &ctx.accounts.stake_pda.key(),
            &ctx.accounts.authority.key(),
            reward_available_to_buyback,
            &ctx.accounts.stake_split_pda.key(),
        )[2];
        invoke_signed(
            split_ix,
            &[
                ctx.accounts.stake_pda.to_account_info(),
                split_ai.clone(),
                authority_ai.clone(),
            ],
            &[auth_seeds],
        )?;

        stake_info.buyback_requested = true;
        // The main stake balance just dropped by the split amount; re-baseline so
        // the next accrual check does not misread the decrease.
        stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
        stake_info.last_update_timestamp = block_timestamp;
        return Ok(());
    }

    // From here the split account must already exist and be Stake-program owned.
    require_keys_eq!(
        split_owner,
        ctx.accounts.stake_program.key(),
        ErrorCode::InvalidSplitState
    );
    require!(stake_info.buyback_requested, ErrorCode::NoBuybackRequested);

    // Inspect the split account's delegation to decide the next step.
    let split_state = crate::instructions::load_stake_state(&split_ai)?;
    let current_epoch = Clock::get()?.epoch;

    if let StakeStateV2::Stake(_, stake, _) = split_state {
        let deactivation_epoch = stake.delegation.deactivation_epoch;

        // =================================================================
        // STEP 2 — DEACTIVATE the split account (separate tx from the split).
        // `deactivation_epoch == u64::MAX` means it has not been deactivated.
        // =================================================================
        if deactivation_epoch == u64::MAX {
            msg!("Buyback step 2: deactivating split stake account");
            let ix = stake_ix::deactivate_stake(
                &ctx.accounts.stake_split_pda.key(),
                &ctx.accounts.authority.key(),
            );
            invoke_signed(
                &ix,
                &[split_ai.clone(), clock_ai.clone(), authority_ai.clone()],
                &[auth_seeds],
            )?;
            stake_info.last_update_timestamp = block_timestamp;
            return Ok(());
        }

        // Deactivation was requested but the stake has not fully cooled down yet.
        // The Stake program also enforces this at withdraw time; this gives a
        // clearer error and stops the state machine from advancing too early.
        require!(
            current_epoch > deactivation_epoch,
            ErrorCode::CooldownInProgress
        );
    }

    // =====================================================================
    // STEP 3 — EXECUTE: withdraw the fully-cooled split account, swap the SOL
    // for LXR on Raydium, route LXR to the reward vault and the fee to the SOL
    // treasury, then finalize the buyback.
    // =====================================================================
    msg!("Buyback step 3: executing swap");

    let recipient_ai = ctx.accounts.owner.to_account_info();
    let system_program_ai = ctx.accounts.system_program.to_account_info();
    let token_program_ai = ctx.accounts.token_program.to_account_info();
    let owner_wsol = ctx.accounts.token_0_account.to_account_info();

    let sol_withdrawan = ctx
        .accounts
        .stake_split_pda
        .lamports()
        .checked_sub(min_rent)
        .unwrap();
    require_gt!(sol_withdrawan, 0);

    // Withdraw the entire split account (the rent reserve returns to the admin).
    let ix = stake_ix::withdraw(
        &ctx.accounts.stake_split_pda.key(),
        &ctx.accounts.authority.key(),
        &ctx.accounts.owner.key(),
        ctx.accounts.stake_split_pda.lamports(), // ALL available
        None,                                    // custodian optional
    );
    invoke_signed(
        &ix,
        &[
            split_ai.clone(),
            recipient_ai.clone(),
            clock_ai.clone(),
            stake_history_ai.clone(),
            stake_config_ai.clone(),
            authority_ai.clone(),
        ],
        &[auth_seeds],
    )?;

    // Move the withdrawn lamports into the admin's WSOL account and sync.
    let ix = transfer(
        &ctx.accounts.owner.key(),
        &ctx.accounts.token_0_account.key(),
        sol_withdrawan,
    );
    invoke(&ix, &[recipient_ai, owner_wsol.clone(), system_program_ai])?;

    let sync_ix = sync_native(&spl_token::id(), &ctx.accounts.token_0_account.key())?;
    invoke(&sync_ix, &[owner_wsol, token_program_ai])?;

    // --- Treasury fee (in SOL/WSOL) ---
    let fee_treasury = (sol_withdrawan as u128)
        .checked_mul(ctx.accounts.global_config.fee_treasury_rate as u128)
        .unwrap()
        .checked_div(FEE_RATE_DENOMINATOR_VALUE as u128)
        .unwrap() as u64;

    // --- Exact-input amount sent to the pool after fee ---
    let actual_amount_in = sol_withdrawan.checked_sub(fee_treasury).unwrap();
    require_gt!(actual_amount_in, 0);

    // --- Read pool state + compute pricing ---
    let pool_state_info = &ctx.accounts.pool_state;
    let pool_state = PoolState::try_deserialize(&mut &pool_state_info.data.borrow()[..])?;
    let SwapParams {
        trade_direction: _,
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

    let creator_fee_rate = pool_state.adjust_creator_fee_rate(500);

    // Price the exact-input trade.
    let result = CurveCalculator::swap_base_input(
        u128::from(actual_amount_in),
        u128::from(total_input_token_amount),
        u128::from(total_output_token_amount),
        2500, // base fee (example)
        creator_fee_rate,
        120000, // price impact limit (example)
        40000,  // oracle/other adjustment (example)
        is_creator_fee_on_input,
    )
    .ok_or(ErrorCode::ZeroTradingTokens)?;

    require_eq!(u64::try_from(result.input_amount).unwrap(), actual_amount_in);

    // Output LXR expected from the priced trade (verified by the Raydium CPI).
    let lxr_bought = u64::try_from(result.output_amount).unwrap();

    stake_info.total_luxor_rewards_accrued = stake_info
        .total_luxor_rewards_accrued
        .checked_add(lxr_bought)
        .unwrap();
    stake_info.total_sol_used_for_buyback = stake_info
        .total_sol_used_for_buyback
        .checked_add(sol_withdrawan)
        .unwrap();

    stake_info.last_buyback_timestamp = block_timestamp;
    stake_info.reward_per_token_lxr_stored = stake_info
        .reward_per_token_lxr_stored
        .checked_add(
            (lxr_bought as u128)
                .checked_mul(PRECISION)
                .unwrap()
                .checked_div(stake_info.total_staked_sol as u128)
                .unwrap(),
        )
        .unwrap();

    // --- Build Raydium `swap_base_input` CPI payload (discriminator + params) ---
    let params = SwapBaseInput {
        amount_in: actual_amount_in,
        minimum_amount_out: 0, // accept any positive amount
    };

    // Discriminator for `global:swap_base_input` (Raydium CPMM)
    let discriminator =
        anchor_lang::solana_program::hash::hash(b"global:swap_base_input").to_bytes()[..8].to_vec();
    let mut data = discriminator;
    data.extend(params.try_to_vec()?);

    let accounts = vec![
        AccountMeta::new(ctx.accounts.owner.key(), true),
        AccountMeta::new_readonly(ctx.accounts.raydium_authority.key(), false),
        AccountMeta::new_readonly(ctx.accounts.amm_config.key(), false),
        AccountMeta::new(ctx.accounts.pool_state.key(), false),
        AccountMeta::new(ctx.accounts.token_0_account.key(), false),
        AccountMeta::new(ctx.accounts.token_1_account.key(), false),
        AccountMeta::new(ctx.accounts.token_0_vault.key(), false),
        AccountMeta::new(ctx.accounts.token_1_vault.key(), false),
        AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
        AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
        AccountMeta::new_readonly(ctx.accounts.vault_0_mint.key(), false),
        AccountMeta::new_readonly(ctx.accounts.vault_1_mint.key(), false),
        AccountMeta::new(ctx.accounts.observation_state.key(), false),
    ];

    let ix = Instruction {
        program_id: crate::raydium_cpmm::id(),
        accounts,
        data,
    };

    // Execute the Raydium CPMM swap.
    let accounts = Box::new(vec![
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.raydium_authority.to_account_info(),
        ctx.accounts.amm_config.to_account_info(),
        ctx.accounts.pool_state.to_account_info(),
        ctx.accounts.token_0_account.to_account_info(),
        ctx.accounts.token_1_account.to_account_info(),
        ctx.accounts.token_0_vault.to_account_info(),
        ctx.accounts.token_1_vault.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.vault_0_mint.to_account_info(),
        ctx.accounts.vault_1_mint.to_account_info(),
        ctx.accounts.observation_state.to_account_info(),
    ]);

    invoke(&ix, &*accounts)?;

    // --- Settle post-swap balances ---

    // Send acquired LXR (token_1) to the LXR reward vault.
    transfer_from_user_to_pool_vault(
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_1_account.to_account_info(),
        ctx.accounts.luxor_reward_vault.to_account_info(),
        ctx.accounts.vault_1_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        lxr_bought,
        ctx.accounts.vault_1_mint.decimals,
    )?;

    // Send the treasury fee (token_0 / WSOL) to the SOL treasury vault.
    transfer_from_user_to_pool_vault(
        ctx.accounts.owner.to_account_info(),
        ctx.accounts.token_0_account.to_account_info(),
        ctx.accounts.sol_treasury_vault.to_account_info(),
        ctx.accounts.vault_0_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        fee_treasury,
        ctx.accounts.vault_0_mint.decimals,
    )?;

    // --- Event for indexers / analytics ---
    emit!(BuybackExecuted {
        sol_amount: sol_withdrawan,
        lxr_bought,
        fee_to_treasury: fee_treasury,
    });

    stake_info.buyback_requested = false;
    stake_info.buyback_count = stake_info.buyback_count.checked_add(1).unwrap();
    stake_info.last_update_timestamp = block_timestamp;

    Ok(())
}
