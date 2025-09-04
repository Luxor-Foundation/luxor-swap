use crate::curve::CurveCalculator;
use crate::curve::FEE_RATE_DENOMINATOR_VALUE;
use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::transfer_from_user_to_pool_vault;
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
    #[account(address = crate::luxor_pool_state::id())]
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

    /// SPL Token program (used both for WSOL sync and token transfers).
    pub token_program: Program<'info, Token>,

    /// Associated Token Program (for creating ATAs as needed).
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// System Program (for SOL transfers from stake PDA).
    pub system_program: Program<'info, System>,
}

/// Executes a buyback of LXR using **stake rewards in SOL**, then routes:
/// - LXR bought → `luxor_reward_vault`
/// - Fee in SOL/WSOL → `sol_treasury_vault`
///
/// ## Steps & Invariants
/// - Accrual: Realizes any delta SOL in `stake_pda` into `stake_info` and updates
///   `reward_per_token_sol_stored` with `PRECISION / total_staked_sol`.
/// - Budget: `reward_available_to_buyback = total_sol_rewards_accrued - total_sol_used_for_buyback`.
/// - Movement: Transfers `reward_available_to_buyback` lamports from `stake_pda` to admin's
///   `token_0_account` (native SOL → WSOL), then `sync_native`.
/// - Fee: `fee_treasury = reward_available_to_buyback * fee_treasury_rate / FEE_RATE_DENOMINATOR_VALUE`.
/// - Trade: For `actual_amount_in = reward_available_to_buyback - fee_treasury`, compute exact-input
///   swap via `CurveCalculator::swap_base_input`. Check:
///     * `constant_after >= constant_before`
///     * `result.input_amount == actual_amount_in`
/// - CPI: Call Raydium `swap_base_input` with a constructed discriminator+payload.
/// - Settlement: Move LXR output to reward vault; move SOL fee to SOL treasury vault.
/// - State: Update `total_luxor_rewards_accrued`, `total_sol_used_for_buyback`,
///   `reward_per_token_lxr_stored`, timestamps; emit `BuybackExecuted`.
pub fn buyback(ctx: Context<Buyback>) -> Result<()> {
    let stake_info = &mut ctx.accounts.stake_info;
    let stake_split_pda = &ctx.accounts.stake_split_pda;
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;
    if stake_info.buyback_requested {
        require_keys_eq!(*stake_split_pda.owner, ctx.accounts.stake_program.key());
        require!(stake_info.buyback_requested, ErrorCode::NoBuybackRequested);
        let stake_account = ctx.accounts.stake_split_pda.to_account_info();
        let recipient_ai = ctx.accounts.owner.to_account_info();
        let system_program = ctx.accounts.system_program.to_account_info();
        let token_program = ctx.accounts.token_program.to_account_info();
        let authority_ai = ctx.accounts.authority.to_account_info();
        let clock_ai = ctx.accounts.clock.to_account_info();
        let owner_wsol = ctx.accounts.token_0_account.to_account_info();
        let owner_ai = ctx.accounts.owner.to_account_info();

        let space = size_of::<StakeStateV2>();
        let min_rent = Rent::get()?.minimum_balance(space);
        require!(min_rent > 0, ErrorCode::InsufficientRent);    

        let sol_withdrawan = ctx.accounts.stake_split_pda.lamports().checked_sub(min_rent).unwrap();   

        let ix = stake_ix::withdraw(
            &stake_account.key(),
            &ctx.accounts.authority.key(),
            &ctx.accounts.owner.key(),
            ctx.accounts.stake_split_pda.lamports(),   // u64, or ALL available
            None,       // custodian optional
        );

        let auth_bump = ctx.bumps.authority;
        let seeds: &[&[u8]] = &[crate::AUTH_SEED.as_bytes(), &[auth_bump]];
        invoke_signed(&ix, &[stake_account, authority_ai, recipient_ai, clock_ai], &[seeds])?;

        let ix = transfer(
            &ctx.accounts.owner.key(),
            &ctx.accounts.token_0_account.key(),
            sol_withdrawan,
        );

        invoke(&ix, &[owner_ai, owner_wsol.clone(), system_program])?;

        // Convert the lamports just transferred into WSOL token balance.
        let sync_ix = sync_native(&spl_token::id(), &ctx.accounts.token_0_account.key())?;
        invoke(&sync_ix, &[owner_wsol, token_program.clone()])?;

        // --- Treasury fee (in SOL/WSOL) ---
        let fee_treasury = (sol_withdrawan as u128)
            .checked_mul(ctx.accounts.global_config.fee_treasury_rate as u128)
            .unwrap()
            .checked_div(FEE_RATE_DENOMINATOR_VALUE as u128)
            .unwrap() as u64;
        require!(fee_treasury > 0, ErrorCode::ZeroTradingTokens);

        // --- Exact-input amount sent to the pool after fee ---
        let actual_amount_in = sol_withdrawan
            .checked_sub(fee_treasury)
            .unwrap();
        require_gt!(actual_amount_in, 0);

        // --- Read pool state + compute pricing invariants ---
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

        let constant_before = u128::from(total_input_token_amount)
            .checked_mul(u128::from(total_output_token_amount))
            .unwrap();

        let creator_fee_rate = pool_state.adjust_creator_fee_rate(500);

        // Price the exact-input trade and validate invariants.
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

        let constant_after = u128::from(result.new_input_vault_amount)
            .checked_mul(u128::from(result.new_output_vault_amount))
            .unwrap();

        require_eq!(
            u64::try_from(result.input_amount).unwrap(),
            actual_amount_in
        );
        require_gte!(constant_after, constant_before);

        // Output LXR expected from the priced trade (will later be verified by Raydium CPI).
        let lxr_bought = u64::try_from(result.output_amount).unwrap();

        stake_info.total_luxor_rewards_accrued = stake_info
            .total_luxor_rewards_accrued
            .checked_add(lxr_bought)
            .unwrap();
        stake_info.total_sol_used_for_buyback = stake_info
            .total_sol_used_for_buyback
            .checked_add(actual_amount_in)
            .unwrap();
    
        stake_info.last_buyback_timestamp = block_timestamp;
        stake_info.reward_per_token_lxr_stored = stake_info
            .reward_per_token_lxr_stored
            .checked_add(
                (lxr_bought as u128)
                    .checked_mul(PRECISION)
                    .unwrap()
                    .checked_div(stake_info.total_staked_sol as u128)
                    .unwrap()).unwrap();

        // --- Build Raydium `swap_base_input` CPI payload (Anchor-style discriminator + params) ---
        let params = SwapBaseInput {
            amount_in: actual_amount_in,
            minimum_amount_out: 0, // accept any positive amount; slippage bounded by invariant checks above
        };

        // Discriminator for `global:swap_base_input` (Raydium CPMM)
        let discriminator =
            anchor_lang::solana_program::hash::hash(b"global:swap_base_input").to_bytes()[..8].to_vec();
        let mut data = discriminator;
        data.extend(params.try_to_vec()?);

        // CPI account metas expected by Raydium CPMM
        let payer = ctx.accounts.owner.key();
        let raydium_authority = ctx.accounts.raydium_authority.key();
        let amm_config = ctx.accounts.amm_config.key();
        let pool_state = ctx.accounts.pool_state.key();
        let input_token_account = ctx.accounts.token_0_account.key();
        let output_token_account = ctx.accounts.token_1_account.key();
        let input_vault = ctx.accounts.token_0_vault.key();
        let output_vault = ctx.accounts.token_1_vault.key();
        let input_output_token_program = ctx.accounts.token_program.key();
        let input_token_mint = ctx.accounts.vault_0_mint.key();
        let output_token_mint = ctx.accounts.vault_1_mint.key();
        let observation_state = ctx.accounts.observation_state.key();

        let accounts = vec![
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(raydium_authority, false),
            AccountMeta::new_readonly(amm_config, false),
            AccountMeta::new(pool_state, false),
            AccountMeta::new(input_token_account, false),
            AccountMeta::new(output_token_account, false),
            AccountMeta::new(input_vault, false),
            AccountMeta::new(output_vault, false),
            AccountMeta::new_readonly(input_output_token_program, false),
            AccountMeta::new_readonly(input_output_token_program, false),
            AccountMeta::new_readonly(input_token_mint, false),
            AccountMeta::new_readonly(output_token_mint, false),
            AccountMeta::new(observation_state, false),
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

        stake_info.buyback_requested = false;
        stake_info.buyback_count = stake_info.buyback_count.checked_add(1).unwrap();

        // --- Event for indexers / analytics ---
        emit!(BuybackExecuted {
            sol_amount: sol_withdrawan,
            lxr_bought,
            fee_to_treasury: fee_treasury,
        });        
    
    } else {
        require_keys_eq!(*stake_split_pda.owner, ctx.accounts.system_program.key());
        require!(!stake_info.buyback_requested, ErrorCode::BuybackAlreadyRequested);

        let payer = ctx.accounts.owner.to_account_info();
        let stake_ai = ctx.accounts.stake_pda.to_account_info();
        let stake_pda_ai = ctx.accounts.stake_split_pda.to_account_info();
        let system_program_ai = ctx.accounts.system_program.to_account_info();
        let authority = ctx.accounts.authority.to_account_info();  
        let clock_ai = ctx.accounts.clock.to_account_info();  
        // Compute rent-exempt minimum lamports for StakeStateV2.
        let space = size_of::<StakeStateV2>();
        let min_rent = Rent::get()?.minimum_balance(space);
        require!(min_rent > 0, ErrorCode::InsufficientRent);    

        // Derive seeds for stake account PDA.
        let bump = ctx.bumps.stake_split_pda;
        let stake_seeds: &[&[u8]] = &[STAKE_SPLIT_ACCOUNT_SEED.as_bytes(), &stake_info.buyback_count.to_le_bytes(), &[bump]];

        // 2a) Create Stake account with owner = Stake program
        let create_ix = system_instruction::create_account(
            &payer.key(),
            &stake_split_pda.key(),
            min_rent,
            space as u64,
            &stake::program::ID,
        );

        invoke_signed(
            &create_ix,
            &[payer.clone(), stake_pda_ai.clone(), system_program_ai],
            &[stake_seeds],
        )?;

        // --- Accrue any newly observed SOL rewards on the stake PDA ---
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
            stake_info.reward_per_token_sol_stored = stake_info
                .reward_per_token_sol_stored
                .checked_add(
                       (rewards_accured as u128)
                        .checked_mul(PRECISION)
                        .unwrap()
                        .checked_div(stake_info.total_staked_sol as u128)
                        .unwrap()).unwrap();
        }

        // --- Available rewards (SOL) to use for buyback ---
        let reward_available_to_buyback = stake_info
            .total_sol_rewards_accrued
            .checked_sub(stake_info.total_sol_used_for_buyback).unwrap();

        let ix = &stake_ix::split(
            &stake_ai.key(),            // source stake
            &authority.key(),            // stake authority PDA
            reward_available_to_buyback,     // rewards you computed
            &stake_pda_ai.key(),            // destination stake account (rent-exempt, stake-owned)
        )[2];
        let auth_bump = ctx.bumps.authority;
        let seeds: &[&[u8]] = &[crate::AUTH_SEED.as_bytes(), &[auth_bump]];
        invoke_signed(ix, &[stake_ai, stake_pda_ai.clone(), authority.clone()], &[seeds])?;     

        let ix = stake_ix::deactivate_stake(&stake_pda_ai.key(), &authority.key());
        invoke_signed(&ix, &[stake_pda_ai, clock_ai, authority], &[seeds])?;

        stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
        stake_info.buyback_requested = true;
        stake_info.last_update_timestamp = block_timestamp;

    }

    
    Ok(())
}
