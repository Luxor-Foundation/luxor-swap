use crate::error::ErrorCode;
use crate::states::*;
use crate::AUTH_SEED;
use crate::DEPOSIT_STAKE_ACCOUNT_SEED;
use crate::STAKE_ACCOUNT_SEED;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_lang::solana_program::program::invoke;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::stake;
use anchor_lang::solana_program::stake::instruction as stake_ix;
use anchor_lang::solana_program::stake::state::{Authorized, Lockup, StakeStateV2};
use anchor_lang::solana_program::system_instruction;
use anchor_lang::solana_program::sysvar;
use std::mem::size_of;

/// Accounts required to fold newly deposited (un-delegated) SOL into the main
/// stake so it starts earning, WITHOUT unstaking anything.
///
/// ## Why this instruction exists
/// When a user swaps SOL→LXR, `purchase()` transfers their SOL into the main
/// stake PDA but only delegates it when the stake is not already active
/// (Solana rejects re-delegating an active stake with `TooSoonToRedelegate`).
/// The main stake is always active, so those deposits pile up as **un-delegated
/// excess lamports** and never earn. The old buyback used to sweep them in by
/// deactivating and re-delegating the whole stake — the ~2-day unstaking that
/// the 3-step buyback rewrite deliberately removed.
///
/// The only way to add lamports to an already-active stake without unstaking is
/// to delegate them as a **separate** stake account and then **merge** it into
/// the main stake once it is fully active. That is a 2-step, cross-epoch flow,
/// so this instruction is a small state machine advancing one step per call,
/// driven by the on-chain state of the transient `deposit_stake_pda`:
///
/// - **Step 1 — Stake** (`deposit_stake_pda` still System-owned == not created):
///   withdraw the un-delegated excess out of the main stake into a fresh
///   transient stake account and delegate it to the same validator.
/// - **Step 2 — Merge** (`deposit_stake_pda` Stake-owned and fully active):
///   merge it back into the main stake (this drains and closes it), so the
///   deposits are now part of the delegated principal.
///
/// Between the two steps the caller waits ~1 epoch for the transient stake to
/// finish warming up; calling Step 2 early returns `DepositStakeNotReady`.
///
/// ## Who can call it
/// The protocol `admin` (manual fallback via the admin panel) OR the dedicated
/// low-privilege `keeper` key that an off-chain scheduler uses to run this
/// automatically each epoch. The keeper key can do nothing else in the program.
#[derive(Accounts)]
pub struct StakeDeposits<'info> {
    /// Admin signer, hardcoded program admin, OR the dedicated keeper key.
    /// The keeper key is authorized ONLY for this instruction — it has no other
    /// power in the program (see `crate::keeper`).
    #[account(
        mut,
        constraint = (
            owner.key() == global_config.admin
            || owner.key() == crate::admin::id()
            || owner.key() == crate::keeper::id()
        ) @ ErrorCode::InvalidOwner
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

    /// Validator vote account the deposits are delegated to (must match config).
    ///
    /// CHECK: pinned to `global_config.vote_account`; validated by the Stake CPI.
    #[account(address = global_config.vote_account)]
    pub vote_account: UncheckedAccount<'info>,

    /// The main protocol stake PDA (holds the un-delegated deposit excess).
    ///
    /// CHECK: PDA seeds ensure derivation; owned by the Stake program.
    #[account(
        mut,
        seeds = [STAKE_ACCOUNT_SEED.as_bytes()],
        bump
    )]
    pub stake_pda: UncheckedAccount<'info>,

    /// Transient stake account the deposits are staked in before being merged.
    /// A single fixed-seed account: the flow always fully creates→delegates→
    /// merges→closes it before the next cycle, so no per-cycle counter (and no
    /// `StakeInfo` layout change / migration) is needed.
    ///
    /// CHECK: PDA seeds ensure derivation; System-owned before Step 1, then
    /// Stake-owned until Step 2 merges and closes it.
    #[account(
        mut,
        seeds = [DEPOSIT_STAKE_ACCOUNT_SEED.as_bytes()],
        bump
    )]
    pub deposit_stake_pda: UncheckedAccount<'info>,

    /// Program authority PDA — the staker/withdrawer for all protocol stake
    /// accounts, and the signer for every Stake CPI here.
    ///
    /// CHECK: PDA derivation enforced by seeds.
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// CHECK: Stake program ID (CPI target).
    #[account(address = stake::program::ID)]
    pub stake_program: UncheckedAccount<'info>,

    /// Clock sysvar (typed — also used to read the current epoch).
    pub clock: Sysvar<'info, Clock>,

    /// Stake history sysvar (typed — used to compute activation status).
    pub stake_history: Sysvar<'info, StakeHistory>,

    /// CHECK: Stake config account (fixed address), required by `delegate_stake`.
    #[account(address = solana_program::stake::config::ID)]
    pub stake_config: UncheckedAccount<'info>,

    /// CHECK: Rent sysvar, required by the Stake `initialize` CPI.
    #[account(address = sysvar::rent::ID)]
    pub rent: UncheckedAccount<'info>,

    /// System Program (creates the transient stake account).
    pub system_program: Program<'info, System>,
}

/// Fold un-delegated deposits into the main stake. One step per call; see the
/// `StakeDeposits` doc comment for the full rationale.
pub fn stake_deposits(ctx: Context<StakeDeposits>) -> Result<()> {
    let stake_info = &mut ctx.accounts.stake_info;
    let block_timestamp = solana_program::clock::Clock::get()?.unix_timestamp as u64;
    let space = size_of::<StakeStateV2>();
    let min_rent = Rent::get()?.minimum_balance(space);
    require!(min_rent > 0, ErrorCode::InsufficientRent);

    // --- Realize any newly accrued SOL rewards on the main stake first ---
    // (identical to the buyback/purchase accrual check). Rewards compound into
    // the delegated stake, so they raise the main PDA's lamports; capture that
    // increase as rewards BEFORE we move any lamports around below.
    if ctx.accounts.stake_pda.lamports() > stake_info.last_tracked_sol_balance {
        let rewards_accrued = ctx
            .accounts
            .stake_pda
            .lamports()
            .checked_sub(stake_info.last_tracked_sol_balance)
            .unwrap();
        stake_info.total_sol_rewards_accrued = stake_info
            .total_sol_rewards_accrued
            .checked_add(rewards_accrued)
            .unwrap();
        stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
    }

    // PDA signer seeds for the program authority (staker/withdrawer for all CPIs).
    let auth_bump = ctx.bumps.authority;
    let auth_seeds: &[&[u8]] = &[AUTH_SEED.as_bytes(), &[auth_bump]];

    let authority_ai = ctx.accounts.authority.to_account_info();
    let clock_ai = ctx.accounts.clock.to_account_info();
    let stake_history_ai = ctx.accounts.stake_history.to_account_info();
    let stake_config_ai = ctx.accounts.stake_config.to_account_info();
    let stake_pda_ai = ctx.accounts.stake_pda.to_account_info();
    let deposit_ai = ctx.accounts.deposit_stake_pda.to_account_info();

    let deposit_owner = *ctx.accounts.deposit_stake_pda.owner;

    // =====================================================================
    // STEP 1 — STAKE (transient account still System-owned => not created).
    // Withdraw the un-delegated excess out of the main stake into a fresh
    // transient stake account, then delegate it. The main stake's delegation
    // (principal + compounded rewards) is never touched.
    // =====================================================================
    if deposit_owner == ctx.accounts.system_program.key() {
        // Un-delegated excess = main lamports - delegated stake - rent reserve.
        // Rewards live inside `delegation.stake`, so this figure is purely the
        // deposits that `purchase()` could not delegate — never the rewards.
        let main_state = crate::instructions::load_stake_state(&stake_pda_ai)?;
        let (main_rent_reserve, main_delegated) = match main_state {
            StakeStateV2::Stake(meta, stake, _) => {
                (meta.rent_exempt_reserve, stake.delegation.stake)
            }
            StakeStateV2::Initialized(meta) => (meta.rent_exempt_reserve, 0),
            _ => return Err(ErrorCode::InvalidStakeAccountData.into()),
        };

        let excess = ctx
            .accounts
            .stake_pda
            .lamports()
            .checked_sub(main_delegated)
            .unwrap()
            .checked_sub(main_rent_reserve)
            .unwrap();
        require!(excess > 0, ErrorCode::NoDepositsToStake);
        msg!("Stake-deposits step 1: staking excess = {}", excess);

        // 1a) Create the transient stake account (owned by the Stake program).
        //     `owner` fronts the rent reserve; it ends up added to the main
        //     stake on merge (a negligible protocol contribution per cycle).
        let deposit_bump = ctx.bumps.deposit_stake_pda;
        let deposit_seeds: &[&[u8]] = &[DEPOSIT_STAKE_ACCOUNT_SEED.as_bytes(), &[deposit_bump]];
        let create_ix = system_instruction::create_account(
            &ctx.accounts.owner.key(),
            &ctx.accounts.deposit_stake_pda.key(),
            min_rent,
            space as u64,
            &stake::program::ID,
        );
        invoke_signed(
            &create_ix,
            &[
                ctx.accounts.owner.to_account_info(),
                deposit_ai.clone(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[deposit_seeds],
        )?;

        // 1b) Initialize it with the SAME authorities + lockup as the main stake
        //     (staker = withdrawer = authority PDA, default lockup). Matching
        //     these is required for the later merge to be accepted.
        let authorized = Authorized {
            staker: ctx.accounts.authority.key(),
            withdrawer: ctx.accounts.authority.key(),
        };
        let init_ix = stake_ix::initialize(
            &ctx.accounts.deposit_stake_pda.key(),
            &authorized,
            &Lockup::default(),
        );
        invoke(&init_ix, &[deposit_ai.clone(), ctx.accounts.rent.to_account_info()])?;

        // 1c) Withdraw the excess out of the main stake into the transient one.
        //     Only the un-delegated portion is withdrawable, so the delegated
        //     principal + rewards stay put in the main stake.
        let withdraw_ix = stake_ix::withdraw(
            &ctx.accounts.stake_pda.key(),
            &ctx.accounts.authority.key(),
            &ctx.accounts.deposit_stake_pda.key(),
            excess,
            None,
        );
        invoke_signed(
            &withdraw_ix,
            &[
                stake_pda_ai.clone(),
                deposit_ai.clone(),
                clock_ai.clone(),
                stake_history_ai.clone(),
                authority_ai.clone(),
            ],
            &[auth_seeds],
        )?;

        // 1d) Delegate the transient stake to the same validator. It now warms
        //     up over ~1 epoch; Step 2 merges it in once fully active.
        let delegate_ix = stake_ix::delegate_stake(
            &ctx.accounts.deposit_stake_pda.key(),
            &ctx.accounts.authority.key(),
            &ctx.accounts.vote_account.key(),
        );
        invoke_signed(
            &delegate_ix,
            &[
                deposit_ai.clone(),
                ctx.accounts.vote_account.to_account_info(),
                clock_ai.clone(),
                stake_history_ai.clone(),
                stake_config_ai.clone(),
                authority_ai.clone(),
            ],
            &[auth_seeds],
        )?;

        // The main stake's balance just dropped by `excess`; re-baseline so the
        // next accrual check does not misread the decrease. `total_staked_sol`
        // is unchanged — deposits were already counted there at purchase time;
        // we only moved them from un-delegated to delegated.
        stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
        stake_info.last_update_timestamp = block_timestamp;
        return Ok(());
    }

    // From here the transient account must exist and be Stake-program owned.
    require_keys_eq!(
        deposit_owner,
        ctx.accounts.stake_program.key(),
        ErrorCode::InvalidDepositStakeState
    );

    // =====================================================================
    // STEP 2 — MERGE the fully-active transient stake back into the main
    // stake. A merge is only accepted once the transient stake has finished
    // warming up (both accounts fully active, same validator/authorities).
    // Merging drains the transient account to zero, so the runtime purges it
    // and the fixed seed is free for the next cycle.
    // =====================================================================
    let deposit_state = crate::instructions::load_stake_state(&deposit_ai)?;
    let current_epoch = ctx.accounts.clock.epoch;

    if let StakeStateV2::Stake(_, deposit_stake, _) = deposit_state {
        // Fully active == nothing still warming up and some effective stake.
        let status = deposit_stake.delegation.stake_activating_and_deactivating(
            current_epoch,
            &*ctx.accounts.stake_history,
            None,
        );
        require!(
            status.activating == 0 && status.effective > 0,
            ErrorCode::DepositStakeNotReady
        );
    } else {
        return Err(ErrorCode::InvalidDepositStakeState.into());
    }

    msg!("Stake-deposits step 2: merging transient stake into main stake");
    let merge_ixs = stake_ix::merge(
        &ctx.accounts.stake_pda.key(),
        &ctx.accounts.deposit_stake_pda.key(),
        &ctx.accounts.authority.key(),
    );
    invoke_signed(
        &merge_ixs[0],
        &[
            stake_pda_ai.clone(),
            deposit_ai.clone(),
            clock_ai.clone(),
            stake_history_ai.clone(),
            authority_ai.clone(),
        ],
        &[auth_seeds],
    )?;

    // The merged lamports are principal, not rewards — re-baseline so the next
    // accrual check does not miscount the main stake's increase as reward.
    stake_info.last_tracked_sol_balance = ctx.accounts.stake_pda.lamports();
    stake_info.last_update_timestamp = block_timestamp;
    Ok(())
}
