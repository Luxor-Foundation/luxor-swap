use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct EmergencyWithdraw<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn emergency_withdraw(ctx: Context<EmergencyWithdraw>, param: u8) -> Result<()> {
    Ok(())
}
