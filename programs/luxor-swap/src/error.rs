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
}
