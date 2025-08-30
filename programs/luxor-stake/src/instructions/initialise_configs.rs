use crate::error::ErrorCode;
use crate::{states::*, LUXOR_REWARD_VAULT_SEED, LUXOR_VAULT_SEED, STAKE_ACCOUNT_SEED};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::stake::instruction as stake_ix;
use anchor_lang::solana_program::stake::state::{Authorized, Lockup, StakeStateV2};
use anchor_lang::solana_program::{stake, system_instruction};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use std::mem::size_of;
use std::ops::DerefMut;

/// Initializes global protocol configuration, mints the vault ATA,
/// and **creates + initializes** a Stake account at a PDA derived from `STAKE_ACCOUNT_SEED`.
///
/// The Stake account is created with **owner = Stake program** and funded with **rent-exempt minimum only**.
/// It is initialized with `staker` and `withdrawer` set to the program `authority` PDA.
/// No delegation happens here (can be done later).
#[derive(Accounts)]
pub struct InitialiseConfigs<'info> {
    /// Admin signer (must match program-level admin id)
    #[account(
        mut,
        constraint = owner.key() == crate::admin::id() @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// Program authority PDA used as staker/withdrawer on the Stake account.
    ///
    /// CHECK: PDA derivation is enforced by seeds; we additionally rely on it only as a Pubkey
    /// (no data read), so runtime owner is not critical here.
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// Global config account
    #[account(
        init,
        seeds = [GLOBAL_CONFIG_SEED.as_bytes()],
        bump,
        payer = owner,
        space = GlobalConfig::LEN
    )]
    pub global_config: Account<'info, GlobalConfig>,

    /// LUXOR mint (fixed)
    #[account(
        constraint = luxor_mint.key() == crate::luxor_mint::id() @ ErrorCode::InvalidLuxorMint,
        mint::token_program = token_program,
    )]
    pub luxor_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Program-owned token vault for LUXOR
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

    /// Stake account PDA to be created and initialized (owner = Stake program).
    ///
    /// CHECK: PDA derivation enforced by seeds. We perform runtime checks below to ensure
    /// it's not pre-existing and not owned by an unexpected program before creation.
    #[account(
        mut,
        seeds = [STAKE_ACCOUNT_SEED.as_bytes()],
        bump
    )]
    pub stake_pda: UncheckedAccount<'info>,

    /// Token program (interface)
    pub token_program: Interface<'info, TokenInterface>,

    /// Sysvar: Rent (required by Stake::Initialize)
    pub rent: Sysvar<'info, Rent>,

    /// CHECK: Stake program account; id check guards against wrong account being passed.
    #[account(
        mut,
        constraint = stake_program.key() == stake::program::ID @ ErrorCode::InvalidStakeProgram,
    )]
    pub stake_program: UncheckedAccount<'info>,

    /// System program
    pub system_program: Program<'info, System>,
}

pub fn initialise_configs(
    ctx: Context<InitialiseConfigs>,
    admin: Pubkey,
    bonus_rate: u64,
    max_stake_count_to_get_bonus: u64,
    min_swap_amount: u64,
    fee_treasury_rate: u64,
    purchase_enabled: bool,
    redeem_enabled: bool,
) -> Result<()> {
    // ---------------------------
    // 1) Write global config
    // ---------------------------
    let global_config = ctx.accounts.global_config.deref_mut();
    global_config.bump = ctx.bumps.global_config;
    global_config.admin = admin;
    global_config.bonus_rate = bonus_rate;
    global_config.max_stake_count_to_get_bonus = max_stake_count_to_get_bonus;
    global_config.min_swap_amount = min_swap_amount;
    global_config.fee_treasury_rate = fee_treasury_rate;
    global_config.purchase_enabled = purchase_enabled;
    global_config.redeem_enabled = redeem_enabled;
    msg!("Global Config initialized");

    // ---------------------------
    // 2) Create + Initialize Stake PDA
    // ---------------------------
    let payer = ctx.accounts.owner.to_account_info();
    let stake_pda_ai = ctx.accounts.stake_pda.to_account_info();
    let system_program_ai = ctx.accounts.system_program.to_account_info();

    // If the PDA already exists with the Stake program as owner, treat as idempotent and bail out.
    if stake_pda_ai.lamports() > 0 {
        // If it exists but is NOT owned by Stake program, that's a hard error.
        require!(
            *stake_pda_ai.owner == stake::program::ID,
            ErrorCode::InvalidStakePdaOwner
        );
        msg!("Stake PDA already initialized; skipping creation/initialize");
        return Ok(());
    }

    // Before creation, if the account is non-existent (lamports == 0), owner must be SystemProgram (or default).
    // Anchor gives UncheckedAccount with default owner=System; this assert protects against unexpected pre-state.
    require!(
        *stake_pda_ai.owner == system_program_ai.key() || stake_pda_ai.lamports() == 0,
        ErrorCode::InvalidStakePdaOwner
    );

    // Compute rent for StakeStateV2
    let space = size_of::<StakeStateV2>();
    let min_rent = Rent::get()?.minimum_balance(space);
    require!(min_rent > 0, ErrorCode::InsufficientRent);

    // PDA seeds for the stake account
    let bump = ctx.bumps.stake_pda;
    let stake_seeds: &[&[u8]] = &[STAKE_ACCOUNT_SEED.as_bytes(), &[bump]];

    // 2a) Create the Stake account at the PDA with owner=Stake program, funded with rent only
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

    // 2b) Initialize: set authorities to authority PDA; no lockup
    let authorized = Authorized {
        staker: ctx.accounts.authority.key(),
        withdrawer: ctx.accounts.authority.key(),
    };
    let lockup = Lockup::default();

    let init_ix = stake_ix::initialize(&stake_pda_ai.key(), &authorized, &lockup);
    // Stake::Initialize requires: [writable stake account, sysvar::rent]
    anchor_lang::solana_program::program::invoke(
        &init_ix,
        &[
            stake_pda_ai,                        // writable stake account
            ctx.accounts.rent.to_account_info(), // sysvar rent (Anchor will pass the correct account via ctx.accounts.rent as well)
        ],
    )?;

    msg!("Stake PDA created and initialized");
    Ok(())
}
