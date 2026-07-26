use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Not approved")]
    NotApproved,

    #[msg("Input account owner is not the program address")]
    InvalidOwner,

    #[msg("Input token account is empty")]
    EmptySupply,

    #[msg("Invalid input token for swap")]
    InvalidInput,

    #[msg("Address of the provided LP token mint is incorrect")]
    IncorrectLpMint,

    #[msg("Exceeds desired slippage limit")]
    ExceededSlippage,

    #[msg("Given pool token amount results in zero trading tokens")]
    ZeroTradingTokens,

    #[msg("Token-2022 mint extension is not supported")]
    NotSupportMint,

    #[msg("Invalid vault account")]
    InvalidVault,

    #[msg("Initial LP amount is too small (minimum 100 LP tokens required)")]
    InitLpAmountTooLess,

    #[msg("Invalid timestamp conversion")]
    InvalidTimestamp,

    #[msg("Clock sysvar is unavailable")]
    ClockUnavailable,

    #[msg("Arithmetic overflow occurred")]
    Overflow,

    #[msg("This LP is locked permanently and cannot be unlocked")]
    LockIsPermanent,

    #[msg("This LP lock has already been unlocked")]
    LockAlreadyUnlocked,

    #[msg("Unlock time has not yet been reached")]
    UnlockTimeNotReached,

    #[msg("Calculated LP tokens to burn is zero")]
    ZeroLpTokensToBurn,

    #[msg("The provided lock duration exceeds the maximum allowed limit")]
    LockDurationTooLong,

    #[msg("Underflow occurred")]
    UnderflowError,

    #[msg("Zero liquidity in the pool")]
    ZeroLiquidity,

    #[msg("Invalid Luxor mint account")]
    InvalidLuxorMint,

    #[msg("Invalid Stake program account")]
    InvalidStakeProgram,

    #[msg("Stake PDA account already exists")]
    InvalidStakePdaOwner,

    #[msg("Stake PDA account has insufficient rent")]
    InsufficientRent,

    #[msg("Math operation overflowed or underflowed")]
    MathOverflow,

    #[msg("Insufficient vault balance for the operation")]
    InsufficientVault,

    #[msg("Invalid fee model specified")]
    InvalidFeeModel,

    #[msg("No rewards available to claim")]
    NoRewardsToClaim,

    #[msg("Missing remaining account")]
    MissingRemainingAccount,

    #[msg("Invalid parameter provided")]
    InvalidParam,

    #[msg("Purchase functionality is currently disabled")]
    PurchaseDisabled,

    #[msg("Buyback has already been requested")]
    BuybackAlreadyRequested,

    #[msg("No buyback has been requested")]
    NoBuybackRequested,

    #[msg("Invalid stake account data")]
    InvalidStakeAccountData,

    // --- Added for the buyback fix (3-step split/deactivate/execute flow) ---
    // The split stake account exists but is owned by neither the System program
    // (Step 1, not yet created) nor the Stake program (Steps 2/3) — an
    // unexpected state that should never occur in the normal flow.
    #[msg("Split stake account is in an unexpected state")]
    InvalidSplitState,

    // Returned when Step 3 (withdraw + swap) is attempted before the split stake
    // has finished its ~1-epoch deactivation cooldown. The caller must wait one
    // more epoch and retry.
    #[msg("Stake cooldown is still in progress; try again next epoch")]
    CooldownInProgress,

    // --- Added for stake_deposits (delegate new deposits then merge into main) ---
    // The transient deposit-stake account is owned by neither the System program
    // (Step 1, not yet created) nor the Stake program (Step 2) — an unexpected
    // state that should never occur in the normal flow.
    #[msg("Deposit stake account is in an unexpected state")]
    InvalidDepositStakeState,

    // Step 2 (merge) was attempted before the transient deposit stake finished
    // its ~1-epoch warm-up. Wait one more epoch and retry.
    #[msg("Deposit stake is still activating; try again next epoch")]
    DepositStakeNotReady,

    // Step 1 found no un-delegated deposits in the main stake to stake.
    #[msg("No un-delegated deposits available to stake")]
    NoDepositsToStake,
}
