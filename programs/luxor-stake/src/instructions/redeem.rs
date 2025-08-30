use anchor_lang::{prelude::*, solana_program::stake};
use crate::{states::{StakeInfo, UserStakeInfo, USER_STAKE_INFO_SEED}, utils::transfer_from_pool_vault_to_user, LUXOR_REWARD_VAULT_SEED, PRECISION};
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct Redeem<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

     #[account(
        seeds = [
            USER_STAKE_INFO_SEED.as_bytes(), 
            owner.key().as_ref()
        ],
        bump,
    )]
    pub user_stake_info: Account<'info, UserStakeInfo>,

    /// CHECK: This is not dangerous because we don't read or write from this account
    #[account(
        seeds = [crate::AUTH_SEED.as_bytes()],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    pub stake_info: Account<'info, StakeInfo>,

    pub luxor_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        seeds =[LUXOR_REWARD_VAULT_SEED.as_bytes()],
        bump
    )]
    pub luxor_reward_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        constraint = luxor_mint.key() == crate::luxor_mint::id() @ ErrorCode::InvalidLuxorMint,
        mint::token_program = token_program,
    )]
    pub luxor_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        mut,
        token::mint = luxor_mint,
        token::authority = owner,
        token::token_program = token_program,  
    )]
    pub owner_lxr_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Token program (interface)
    pub token_program: Interface<'info, TokenInterface>,


    pub system_program: Program<'info, System>,
}

pub fn redeem(ctx: Context<Redeem>) -> Result<()> {
    let user_stake_info = &mut ctx.accounts.user_stake_info;
    let stake_info = &mut ctx.accounts.stake_info;
    let reward_per_token_lxr_pending = stake_info.reward_per_token_lxr_stored
        .checked_sub(user_stake_info.lxr_reward_per_token_completed)
        .unwrap();
    require!(reward_per_token_lxr_pending > 0, ErrorCode::NoRewardsToClaim);
    let mut lxr_rewards_to_claim = (user_stake_info.total_staked_sol as u128)
        .checked_mul(reward_per_token_lxr_pending).unwrap()
        .checked_div(PRECISION).unwrap()
        .checked_div(PRECISION).unwrap() as u64;
    require!(lxr_rewards_to_claim > 0, ErrorCode::NoRewardsToClaim);
    let mut forfieted_lxr = 0;
    if ctx.accounts.owner_lxr_token.amount < user_stake_info.base_lxr_holdings {
        let lxr_holdings = ctx.accounts.owner_lxr_token.amount;
        let full_rewards = lxr_rewards_to_claim;
        lxr_rewards_to_claim = (lxr_holdings as u128)
            .checked_mul(lxr_rewards_to_claim as u128).unwrap()
            .checked_div(user_stake_info.base_lxr_holdings as u128).unwrap() as u64;
         
        forfieted_lxr = full_rewards.checked_sub(lxr_rewards_to_claim).unwrap(); 

    }
    // user stake info updates
    user_stake_info.total_lxr_claimed = user_stake_info.total_lxr_claimed.checked_add(lxr_rewards_to_claim).unwrap();
    user_stake_info.total_lxr_forfeited = user_stake_info.total_lxr_forfeited.checked_add(forfieted_lxr).unwrap();
    user_stake_info.lxr_reward_per_token_completed = stake_info.reward_per_token_lxr_stored;

    // stake info updates
    stake_info.total_lxr_claimed = stake_info.total_lxr_claimed.checked_add(lxr_rewards_to_claim).unwrap();
    stake_info.total_lxr_forfeited = stake_info.total_lxr_forfeited.checked_add(forfieted_lxr).unwrap();

    // transfer lxr from reward vault to user

    transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.luxor_reward_vault.to_account_info(),
        ctx.accounts.owner_lxr_token.to_account_info(),
        ctx.accounts.luxor_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        lxr_rewards_to_claim,
        ctx.accounts.luxor_mint.decimals,
        &[&[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]]],
    )?;

    // transfer forfieted lxr to treasury
    if forfieted_lxr > 0 {
        transfer_from_pool_vault_to_user(
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.luxor_reward_vault.to_account_info(),
        ctx.accounts.luxor_vault.to_account_info(),
        ctx.accounts.luxor_mint.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        forfieted_lxr,
        ctx.accounts.luxor_mint.decimals,
        &[&[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]]],
        )?;
    }

    // emit event

    Ok(())
}
