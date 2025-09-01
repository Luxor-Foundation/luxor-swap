//! Swap calculations

use crate::curve::{constant_product::ConstantProductCurve, fees::Fees};
use anchor_lang::prelude::*;
use {crate::error::ErrorCode, std::fmt::Debug};

/// Helper function for mapping to ErrorCode::CalculationFailure
pub fn map_zero_to_none(x: u128) -> Option<u128> {
    if x == 0 {
        None
    } else {
        Some(x)
    }
}

/// The direction of a trade, since curves can be specialized to treat each
/// token differently (by adding offsets or weights)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TradeDirection {
    /// Input token 0, output token 1
    ZeroForOne,
    /// Input token 1, output token 0
    OneForZero,
}

/// The direction to round.  Used for pool token to trading token conversions to
/// avoid losing value on any deposit or withdrawal.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoundDirection {
    /// Floor the value, ie. 1.9 => 1, 1.1 => 1, 1.5 => 1
    Floor,
    /// Ceiling the value, ie. 1.9 => 2, 1.1 => 2, 1.5 => 2
    Ceiling,
}

impl TradeDirection {
    /// Given a trade direction, gives the opposite direction of the trade, so
    /// A to B becomes B to A, and vice versa
    pub fn opposite(&self) -> TradeDirection {
        match self {
            TradeDirection::ZeroForOne => TradeDirection::OneForZero,
            TradeDirection::OneForZero => TradeDirection::ZeroForOne,
        }
    }
}

/// Encodes results of depositing both sides at once
#[derive(Debug, PartialEq)]
pub struct TradingTokenResult {
    /// Amount of token A
    pub token_0_amount: u128,
    /// Amount of token B
    pub token_1_amount: u128,
}

/// Encodes all results of swapping from a source token to a destination token
#[derive(Debug, PartialEq)]
pub struct SwapResult {
    /// The new amount in the input token vault, excluding  trade fees
    pub new_input_vault_amount: u128,
    /// The new amount in the output token vault, excluding trade fees
    pub new_output_vault_amount: u128,
    /// User's input amount, including trade fees, excluding transfer fees
    pub input_amount: u128,
    /// The amount to be transfer to user, including transfer fees
    pub output_amount: u128,
    /// Amount of input tokens going to pool holders
    pub trade_fee: u128,
    /// Amount of input tokens going to protocol
    pub protocol_fee: u128,
    /// Amount of input tokens going to protocol team
    pub fund_fee: u128,
    /// Amount of fee tokens going to creator
    pub creator_fee: u128,
}

/// Concrete struct to wrap around the trait object which performs calculation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurveCalculator {}

impl CurveCalculator {
    pub fn validate_supply(token_0_amount: u64, token_1_amount: u64) -> Result<()> {
        if token_0_amount == 0 {
            return Err(ErrorCode::EmptySupply.into());
        }
        if token_1_amount == 0 {
            return Err(ErrorCode::EmptySupply.into());
        }
        Ok(())
    }

    /// Subtract fees and calculate how much destination token will be provided
    /// given an amount of source token.
    pub fn swap_base_input(
        input_amount: u128,
        input_vault_amount: u128,
        output_vault_amount: u128,
        trade_fee_rate: u64,
        creator_fee_rate: u64,
        protocol_fee_rate: u64,
        fund_fee_rate: u64,
        is_creator_fee_on_input: bool,
    ) -> Option<SwapResult> {
        let mut creator_fee = 0;

        let trade_fee = Fees::trading_fee(input_amount, trade_fee_rate)?;
        let input_amount_less_fees = if is_creator_fee_on_input {
            creator_fee = Fees::creator_fee(input_amount, creator_fee_rate)?;
            input_amount
                .checked_sub(trade_fee)?
                .checked_sub(creator_fee)?
        } else {
            input_amount.checked_sub(trade_fee)?
        };
        let protocol_fee = Fees::protocol_fee(trade_fee, protocol_fee_rate)?;
        let fund_fee = Fees::fund_fee(trade_fee, fund_fee_rate)?;

        let output_amount_swapped = ConstantProductCurve::swap_base_input_without_fees(
            input_amount_less_fees,
            input_vault_amount,
            output_vault_amount,
        );

        let output_amount = if is_creator_fee_on_input {
            output_amount_swapped
        } else {
            creator_fee = Fees::creator_fee(output_amount_swapped, creator_fee_rate)?;
            output_amount_swapped.checked_sub(creator_fee)?
        };

        Some(SwapResult {
            new_input_vault_amount: input_vault_amount.checked_add(input_amount_less_fees)?,
            new_output_vault_amount: output_vault_amount.checked_sub(output_amount_swapped)?,
            input_amount,
            output_amount,
            trade_fee,
            protocol_fee,
            fund_fee,
            creator_fee,
        })
    }

    pub fn swap_base_output(
        output_amount: u128,
        input_vault_amount: u128,
        output_vault_amount: u128,
        trade_fee_rate: u64,
        creator_fee_rate: u64,
        protocol_fee_rate: u64,
        fund_fee_rate: u64,
        is_creator_fee_on_input: bool,
    ) -> Option<SwapResult> {
        let trade_fee: u128;
        let mut creator_fee = 0;

        let actual_output_amount = if is_creator_fee_on_input {
            output_amount
        } else {
            let out_amount_with_creator_fee =
                Fees::calculate_pre_fee_amount(output_amount, creator_fee_rate)?;
            creator_fee = out_amount_with_creator_fee - output_amount;
            out_amount_with_creator_fee
        };

        let input_amount_swapped = ConstantProductCurve::swap_base_output_without_fees(
            actual_output_amount,
            input_vault_amount,
            output_vault_amount,
        );

        let input_amount = if is_creator_fee_on_input {
            let input_amount_with_fee = Fees::calculate_pre_fee_amount(
                input_amount_swapped,
                trade_fee_rate + creator_fee_rate,
            )
            .unwrap();
            let total_fee = input_amount_with_fee - input_amount_swapped;
            creator_fee = Fees::split_creator_fee(total_fee, trade_fee_rate, creator_fee_rate)?;
            trade_fee = total_fee - creator_fee;
            input_amount_with_fee
        } else {
            let input_amount_with_fee =
                Fees::calculate_pre_fee_amount(input_amount_swapped, trade_fee_rate).unwrap();
            trade_fee = input_amount_with_fee - input_amount_swapped;
            input_amount_with_fee
        };
        let protocol_fee = Fees::protocol_fee(trade_fee, protocol_fee_rate)?;
        let fund_fee = Fees::fund_fee(trade_fee, fund_fee_rate)?;
        Some(SwapResult {
            new_input_vault_amount: input_vault_amount.checked_add(input_amount_swapped)?,
            new_output_vault_amount: output_vault_amount.checked_sub(actual_output_amount)?,
            input_amount,
            output_amount,
            trade_fee,
            protocol_fee,
            fund_fee,
            creator_fee,
        })
    }

    /// Get the amount of trading tokens for the given amount of pool tokens,
    /// provided the total trading tokens and supply of pool tokens.
    pub fn lp_tokens_to_trading_tokens(
        lp_token_amount: u128,
        lp_token_supply: u128,
        token_0_vault_amount: u128,
        token_1_vault_amount: u128,
        round_direction: RoundDirection,
    ) -> Option<TradingTokenResult> {
        ConstantProductCurve::lp_tokens_to_trading_tokens(
            lp_token_amount,
            lp_token_supply,
            token_0_vault_amount,
            token_1_vault_amount,
            round_direction,
        )
    }
}
