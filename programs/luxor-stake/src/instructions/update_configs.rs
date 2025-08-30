use crate::error::ErrorCode;
use crate::states::GlobalConfig;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    pub global_config: Account<'info, GlobalConfig>,

    pub system_program: Program<'info, System>,
}

pub fn update_config(ctx: Context<UpdateConfig>, param: u8, value: u64) -> Result<()> {
    let global_config = &mut ctx.accounts.global_config;
    match param {
        0 => {
            let new_admin = *ctx
                .remaining_accounts
                .iter()
                .next()
                .ok_or(error!(ErrorCode::MissingRemainingAccount))?
                .key;
            require_keys_neq!(new_admin, Pubkey::default());
            global_config.admin = new_admin;
        }
        1 => {
            global_config.min_swap_amount = value;
        }
        2 => {
            global_config.max_swap_amount = value;
        }
        3 => {
            global_config.fee_treasury_rate = value;
        }
        4 => {
            global_config.purchase_enabled = value != 0;
        }
        5 => {
            global_config.redeem_enabled = value != 0;
        }
        _ => return Err(error!(ErrorCode::InvalidParam)),
    }
    Ok(())
}
