use crate::error::ErrorCode;
use crate::{
    states::*, LUXOR_REWARD_VAULT_SEED, LUXOR_VAULT_SEED, SOL_TREASURY_VAULT_SEED,
    STAKE_ACCOUNT_SEED,
};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::stake::instruction as stake_ix;
use anchor_lang::solana_program::stake::state::{Authorized, Lockup, StakeStateV2};
use anchor_lang::solana_program::{stake, system_instruction};
use anchor_spl::token::spl_token;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use std::mem::size_of;
use std::ops::DerefMut;

/// Accounts context for `initialise_configs`.
///
/// This handler:
/// - Initializes global protocol configuration.
/// - Creates token vaults for protocol LUXOR tokens and reward distribution.
/// - Creates and initializes a Stake account at a PDA, owned by the Stake program.
/// - Stake account is set with program PDA as both staker and withdrawer authority.
#[derive(Accounts)]
pub struct InitialiseConfigs<'info> {
    /// Admin signer (must match the program-level admin id).
    /// Responsible for funding initialization and ensuring proper authority.
    #[account(
        mut,
        address = crate::admin::id() @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// Program authority PDA, used as staker/withdrawer for Stake accounts.
    ///
    /// CHECK: PDA derivation enforced via seeds. Not read as an account; used as Pubkey.
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// Global configuration account holding protocol parameters.
    #[account(
        init,
        seeds = [GLOBAL_CONFIG_SEED.as_bytes()],
        bump,
        payer = owner,
        space = GlobalConfig::LEN
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// Stores metadata and tracking information for stake account.
    #[account(
        init,
        seeds = [STAKE_INFO_SEED.as_bytes()],
        bump,
        payer = owner,
        space = StakeInfo::LEN
    )]
    pub stake_info: Account<'info, StakeInfo>,

    /// LUXOR mint address (fixed, canonical program mint).
    #[account(
        address = crate::luxor_mint::id() @ ErrorCode::InvalidLuxorMint
    )]
    pub luxor_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        address = spl_token::native_mint::id() @ ErrorCode::InvalidLuxorMint
    )]
    pub native_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Program-owned token vault for protocol treasury (LUXOR tokens).
    #[account(
        init,
        seeds =[LUXOR_VAULT_SEED.as_bytes()],
        bump,
        payer = owner,
        token::mint = luxor_mint,
        token::authority = authority,
        token::token_program = token_program,
    )]
    pub luxor_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Vault for distributing rewards (also holds LUXOR).
    #[account(
        init,
        seeds =[LUXOR_REWARD_VAULT_SEED.as_bytes()],
        bump,
        payer = owner,
        token::mint = luxor_mint,
        token::authority = authority,
        token::token_program = token_program,
    )]
    pub luxor_reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init,
        seeds =[SOL_TREASURY_VAULT_SEED.as_bytes()],
        bump,
        payer = owner,
        token::mint = native_mint,
        token::authority = authority,
        token::token_program = token_program,
    )]
    pub sol_treasury_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Stake account PDA (to be created & initialized).
    ///
    /// CHECK: PDA derivation is enforced by seeds. Runtime checks ensure correct owner.
    #[account(
        mut,
        seeds = [STAKE_ACCOUNT_SEED.as_bytes()],
        bump
    )]
    pub stake_pda: UncheckedAccount<'info>,

    /// Token program interface (required for vault creation).
    pub token_program: Interface<'info, TokenInterface>,

    /// Rent sysvar, required by Stake::Initialize.
    pub rent: Sysvar<'info, Rent>,

    /// Stake program (id check enforced).
    ///
    /// CHECK: Only the program ID is validated, not account data.
    #[account(address = stake::program::ID @ ErrorCode::InvalidStakeProgram)]
    pub stake_program: UncheckedAccount<'info>,

    /// Solana System Program.
    pub system_program: Program<'info, System>,
}

/// Initializes global protocol configuration, LUXOR vaults,
/// and creates + initializes the Stake PDA.
///
/// Steps:
/// 1. Write all protocol configuration parameters to `global_config`.
/// 2. Initialize stake metadata account `stake_info`.
/// 3. Create Stake PDA (if not already existing).
/// 4. Initialize Stake PDA with program authority as both staker & withdrawer.
pub fn initialise_configs(
    ctx: Context<InitialiseConfigs>,
    admin: Pubkey,
    vote_account: Pubkey,
    bonus_rate: u64,
    max_stake_count_to_get_bonus: u64,
    min_swap_amount: u64,
    max_swap_amoumnt: u64,
    fee_treasury_rate: u64,
    purchase_enabled: bool,
    redeem_enabled: bool,
    initial_lxr_allocation_vault: u64,
) -> Result<()> {
    // ---------------------------
    // 1) Write global config
    // ---------------------------
    let global_config = ctx.accounts.global_config.deref_mut();
    global_config.bump = ctx.bumps.global_config;
    global_config.admin = admin;
    global_config.lxr_treasury_vault = ctx.accounts.luxor_vault.key();
    global_config.lxr_reward_vault = ctx.accounts.luxor_reward_vault.key();
    global_config.sol_treasury_vault = ctx.accounts.sol_treasury_vault.key();
    global_config.stake_account = ctx.accounts.stake_pda.key();
    global_config.vote_account = vote_account;
    global_config.stake_info = ctx.accounts.stake_info.key();
    global_config.bonus_rate = bonus_rate;
    global_config.max_stake_count_to_get_bonus = max_stake_count_to_get_bonus;
    global_config.min_swap_amount = min_swap_amount;
    global_config.max_swap_amount = max_swap_amoumnt;
    global_config.fee_treasury_rate = fee_treasury_rate;
    global_config.purchase_enabled = purchase_enabled;
    global_config.redeem_enabled = redeem_enabled;
    global_config.initial_lxr_allocation_vault = initial_lxr_allocation_vault;
    msg!("Global Config initialized");

    // Write bump seed for stake_info metadata
    let stake_info = ctx.accounts.stake_info.deref_mut();
    stake_info.bump = ctx.bumps.stake_info;

    // ---------------------------
    // 2) Create + Initialize Stake PDA
    // ---------------------------
    let payer = ctx.accounts.owner.to_account_info();
    let stake_pda_ai = ctx.accounts.stake_pda.to_account_info();
    let system_program_ai = ctx.accounts.system_program.to_account_info();

    // If account exists already and is owned by Stake program → idempotent return.
    if stake_pda_ai.lamports() > 0 {
        require!(
            *stake_pda_ai.owner == stake::program::ID,
            ErrorCode::InvalidStakePdaOwner
        );
        msg!("Stake PDA already initialized; skipping creation/initialize");
        return Ok(());
    }

    // Validate that the account pre-state is valid (system-owned or uninitialized).
    require!(
        *stake_pda_ai.owner == system_program_ai.key() || stake_pda_ai.lamports() == 0,
        ErrorCode::InvalidStakePdaOwner
    );

    // Compute rent-exempt minimum lamports for StakeStateV2.
    let space = size_of::<StakeStateV2>();
    let min_rent = Rent::get()?.minimum_balance(space);
    require!(min_rent > 0, ErrorCode::InsufficientRent);

    // Derive seeds for stake account PDA.
    let bump = ctx.bumps.stake_pda;
    let stake_seeds: &[&[u8]] = &[STAKE_ACCOUNT_SEED.as_bytes(), &[bump]];

    // 2a) Create Stake account with owner = Stake program
    let create_ix = system_instruction::create_account(
        &payer.key(),
        &stake_pda_ai.key(),
        min_rent,
        space as u64,
        &stake::program::ID,
    );

    invoke_signed(
        &create_ix,
        &[payer.clone(), stake_pda_ai.clone(), system_program_ai],
        &[stake_seeds],
    )?;

    // 2b) Initialize Stake account with authority PDA as both staker & withdrawer.
    let authorized = Authorized {
        staker: ctx.accounts.authority.key(),
        withdrawer: ctx.accounts.authority.key(),
    };
    let lockup = Lockup::default();

    let init_ix = stake_ix::initialize(&stake_pda_ai.key(), &authorized, &lockup);

    // Stake::Initialize requires: [writable stake account, sysvar::rent].
    anchor_lang::solana_program::program::invoke(
        &init_ix,
        &[
            stake_pda_ai,                        // writable stake account
            ctx.accounts.rent.to_account_info(), // sysvar rent
        ],
    )?;

    msg!("Stake PDA created and initialized");

    // Track initial SOL balance in stake_info.
    stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();

    emit!(GlobalConfigInitialized {
        admin,
        luxor_mint: ctx.accounts.luxor_mint.key(),
        lxr_treasury_vault: ctx.accounts.luxor_vault.key(),
        lxr_reward_vault: ctx.accounts.luxor_reward_vault.key(),
        stake_account: ctx.accounts.stake_pda.key(),
        vote_account,
        stake_info: ctx.accounts.stake_info.key(),
        bonus_rate,
        max_stake_count_to_get_bonus,
        min_swap_amount,
        max_swap_amount: max_swap_amoumnt,
        fee_treasury_rate,
        purchase_enabled,
        redeem_enabled,
        initial_lxr_allocation_vault,
    });
    Ok(())
}
