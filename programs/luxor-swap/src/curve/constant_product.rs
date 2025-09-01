//! The Uniswap invariantConstantProductCurve::

use crate::{
    curve::calculator::{RoundDirection, TradingTokenResult},
    utils::CheckedCeilDiv,
};

/// ConstantProductCurve struct implementing CurveCalculator
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConstantProductCurve;

impl ConstantProductCurve {
    /// Constant product swap ensures x * y = constant
    /// The constant product swap calculation, factored out of its class for reuse.
    ///
    /// This is guaranteed to work for all values such that:
    ///  - 1 <= source_vault_amount * destination_vault_amount <= u128::MAX
    ///  - 1 <= source_amount <= u64::MAX
    pub fn swap_base_input_without_fees(
        input_amount: u128,
        input_vault_amount: u128,
        output_vault_amount: u128,
    ) -> u128 {
        // (x + delta_x) * (y - delta_y) = x * y
        // delta_y = (delta_x * y) / (x + delta_x)
        let numerator = input_amount.checked_mul(output_vault_amount).unwrap();
        let denominator = input_vault_amount.checked_add(input_amount).unwrap();
        let output_amount = numerator.checked_div(denominator).unwrap();
        output_amount
    }

    pub fn swap_base_output_without_fees(
        output_amount: u128,
        input_vault_amount: u128,
        output_vault_amount: u128,
    ) -> u128 {
        // (x + delta_x) * (y - delta_y) = x * y
        // delta_x = (x * delta_y) / (y - delta_y)
        let numerator = input_vault_amount.checked_mul(output_amount).unwrap();
        let denominator = output_vault_amount.checked_sub(output_amount).unwrap();
        let input_amount = numerator.checked_ceil_div(denominator).unwrap();
        input_amount
    }

    /// Get the amount of trading tokens for the given amount of pool tokens,
    /// provided the total trading tokens and supply of pool tokens.
    ///
    /// The constant product implementation is a simple ratio calculation for how
    /// many trading tokens correspond to a certain number of pool tokens
    pub fn lp_tokens_to_trading_tokens(
        lp_token_amount: u128,
        lp_token_supply: u128,
        token_0_vault_amount: u128,
        token_1_vault_amount: u128,
        round_direction: RoundDirection,
    ) -> Option<TradingTokenResult> {
        let mut token_0_amount = lp_token_amount
            .checked_mul(token_0_vault_amount)?
            .checked_div(lp_token_supply)?;
        let mut token_1_amount = lp_token_amount
            .checked_mul(token_1_vault_amount)?
            .checked_div(lp_token_supply)?;
        let (token_0_amount, token_1_amount) = match round_direction {
            RoundDirection::Floor => (token_0_amount, token_1_amount),
            RoundDirection::Ceiling => {
                let token_0_remainder = lp_token_amount
                    .checked_mul(token_0_vault_amount)?
                    .checked_rem(lp_token_supply)?;
                // Also check for 0 token A and B amount to avoid taking too much
                // for tiny amounts of pool tokens.  For example, if someone asks
                // for 1 pool token, which is worth 0.01 token A, we avoid the
                // ceiling of taking 1 token A and instead return 0, for it to be
                // rejected later in processing.
                if token_0_remainder > 0 && token_0_amount > 0 {
                    token_0_amount += 1;
                }
                let token_1_remainder = lp_token_amount
                    .checked_mul(token_1_vault_amount)?
                    .checked_rem(lp_token_supply)?;
                if token_1_remainder > 0 && token_1_amount > 0 {
                    token_1_amount += 1;
                }
                (token_0_amount, token_1_amount)
            }
        };
        Some(TradingTokenResult {
            token_0_amount,
            token_1_amount,
        })
    }
}
