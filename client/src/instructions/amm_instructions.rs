use anchor_client::{Client, Cluster};
use anchor_lang::prelude::AccountMeta;
use anyhow::Ok;
use anyhow::Result;
use luxor_swap::luxor_pool_state;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, system_program};

use luxor_swap::accounts as raydium_cp_accounts;
use luxor_swap::instruction as raydium_cp_instructions;
use luxor_swap::raydium_cpmm;
use luxor_swap::vault_and_lp_mint_auth;
use std::rc::Rc;

use crate::instructions::utils::get_admin_stake_info_address;
use crate::instructions::utils::get_amm_config_address;
use crate::instructions::utils::get_authority_address;
use crate::instructions::utils::get_global_config_address;
use crate::instructions::utils::get_luxor_reward_vault_address;
use crate::instructions::utils::get_luxor_vault_address;
use crate::instructions::utils::get_observation_state_address;
use crate::instructions::utils::get_raydium_vault;
use crate::instructions::utils::get_sol_treasury_address;
use crate::instructions::utils::get_stake_info_address;
use crate::instructions::utils::get_stake_pda_address;
use crate::instructions::utils::get_user_stake_info_address;

use super::super::{read_keypair_file, ClientConfig};

pub fn initialise_configs_instr(
    config: &ClientConfig,
    admin: Pubkey,
    vote_account: Pubkey,
    bonus_rate: u64,
    max_stake_count_to_get_bonus: u64,
    min_swap_amount: u64,
    max_swap_amount: u64,
    fee_treasury_rate: u64,
    purchase_enabled: bool,
    redeem_enabled: bool,
    initial_lxr_allocation_vault: u64,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.luxor_swap_program)?;

    let instructions = program
        .request()
        .accounts(raydium_cp_accounts::InitialiseConfigs {
            owner: program.payer(),
            global_config: get_global_config_address(&program.id()),
            stake_info: get_stake_info_address(&program.id()),
            admin_stake_info: get_admin_stake_info_address(&program.id()),
            authority: get_authority_address(&program.id()),
            luxor_mint: luxor_swap::luxor_mint::id(),
            luxor_vault: get_luxor_vault_address(&program.id()),
            luxor_reward_vault: get_luxor_reward_vault_address(&program.id()),
            stake_pda: get_stake_pda_address(&program.id()),
            token_program: spl_token::id(),
            native_mint: spl_token::native_mint::id(),
            sol_treasury_vault: get_sol_treasury_address(&program.id()),
            rent: solana_sdk::sysvar::rent::id(),
            stake_program: solana_sdk::stake::program::id(),
            system_program: system_program::id(),
        })
        .args(raydium_cp_instructions::InitialiseConfigs {
            admin,
            vote_account,
            bonus_rate,
            max_stake_count_to_get_bonus,
            min_swap_amount,
            max_swap_amount,
            fee_treasury_rate,
            purchase_enabled,
            redeem_enabled,
            initial_lxr_allocation_vault,
        })
        .instructions()?;
    Ok(instructions)
}

pub fn update_config_instr(
    config: &ClientConfig,
    param: u8,
    value: u64,
    new_admin: Option<Pubkey>,
) -> anyhow::Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.luxor_swap_program)?;

    let mut ixs = program
        .request()
        .accounts(raydium_cp_accounts::UpdateConfig {
            owner: program.payer(),
            global_config: get_global_config_address(&program.id()),
            system_program: system_program::id(),
        })
        .args(raydium_cp_instructions::UpdateConfig { param, value })
        .instructions()?; // build the instruction(s)

    if let Some(admin) = new_admin {
        ixs[0]
            .accounts
            .push(AccountMeta::new_readonly(admin, false));
    }

    Ok(ixs)
}

pub fn manual_purchase_instr(
    config: &ClientConfig,
    user: Pubkey,
    lxr_purchased: u64,
    sol_spent: u64,
    vote_account: Pubkey,
) -> anyhow::Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.luxor_swap_program)?;

    let ixs = program
        .request()
        .accounts(raydium_cp_accounts::ManualPurchase {
            owner: program.payer(),
            global_config: get_global_config_address(&program.id()),
            user,
            user_stake_info: get_user_stake_info_address(&user, &program.id()),
            stake_info: get_stake_info_address(&program.id()),
            authority: get_authority_address(&program.id()),
            system_program: system_program::id(),
            stake_pda: get_stake_pda_address(&program.id()),
            vote_account,
            stake_program: solana_sdk::stake::program::id(),
            clock: solana_sdk::sysvar::clock::id(),
            stake_history: solana_sdk::sysvar::stake_history::id(),
            stake_config: solana_sdk::stake::config::id(),
        })
        .args(raydium_cp_instructions::ManualPurchase {
            lxr_purchased,
            sol_spent,
        })
        .instructions()?; // build the instruction(s)

    Ok(ixs)
}

pub fn purchase_instr(
    config: &ClientConfig,
    lxr_to_purchase: u64,
    max_sol_amount: u64,
    vote_account: Pubkey,
) -> anyhow::Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.luxor_swap_program)?;

    let ixs = program
        .request()
        .accounts(raydium_cp_accounts::Purchase {
            owner: program.payer(),
            global_config: get_global_config_address(&program.id()),
            user_stake_info: get_user_stake_info_address(&program.payer(), &program.id()),
            stake_info: get_stake_info_address(&program.id()),
            authority: get_authority_address(&program.id()),
            luxor_mint: luxor_swap::luxor_mint::id(),
            luxor_vault: get_luxor_vault_address(&program.id()),
            owner_lxr_token: spl_associated_token_account::get_associated_token_address(
                &program.payer(),
                &luxor_swap::luxor_mint::id(),
            ),
            system_program: system_program::id(),
            stake_pda: get_stake_pda_address(&program.id()),
            vote_account,
            stake_program: solana_sdk::stake::program::id(),
            clock: solana_sdk::sysvar::clock::id(),
            stake_history: solana_sdk::sysvar::stake_history::id(),
            stake_config: solana_sdk::stake::config::id(),
            pool_state: luxor_pool_state::id(),
            token_program: spl_token::id(),
            token_0_vault: get_raydium_vault(&raydium_cpmm::id(), &spl_token::native_mint::id()),
            token_1_vault: get_raydium_vault(&raydium_cpmm::id(), &luxor_swap::luxor_mint::id()),
            associated_token_program: spl_associated_token_account::id(),
        })
        .args(raydium_cp_instructions::Purchase {
            lxr_to_purchase,
            max_sol_amount,
        })
        .instructions()?; // build the instruction(s)

    Ok(ixs)
}

pub fn redeem_instr(config: &ClientConfig) -> anyhow::Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.luxor_swap_program)?;

    let ixs = program
        .request()
        .accounts(raydium_cp_accounts::Redeem {
            owner: program.payer(),
            global_config: get_global_config_address(&program.id()),
            user_stake_info: get_user_stake_info_address(&program.payer(), &program.id()),
            stake_info: get_stake_info_address(&program.id()),
            authority: get_authority_address(&program.id()),
            luxor_mint: luxor_swap::luxor_mint::id(),
            luxor_vault: get_luxor_vault_address(&program.id()),
            owner_lxr_token: spl_associated_token_account::get_associated_token_address(
                &program.payer(),
                &luxor_swap::luxor_mint::id(),
            ),
            system_program: system_program::id(),
            associated_token_program: spl_associated_token_account::id(),
            token_program: spl_token::id(),
            luxor_reward_vault: get_luxor_reward_vault_address(&program.id()),
        })
        .args(raydium_cp_instructions::Redeem {})
        .instructions()?; // build the instruction(s)

    Ok(ixs)
}

pub fn buyback_instr(config: &ClientConfig) -> anyhow::Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.luxor_swap_program)?;

    let ixs = program
        .request()
        .accounts(raydium_cp_accounts::Buyback {
            owner: program.payer(),
            global_config: get_global_config_address(&program.id()),
            luxor_reward_vault: get_luxor_reward_vault_address(&program.id()),
            sol_treasury_vault: get_sol_treasury_address(&program.id()),
            stake_info: get_stake_info_address(&program.id()),
            system_program: system_program::id(),
            stake_pda: get_stake_pda_address(&program.id()),
            pool_state: luxor_pool_state::id(),
            token_program: spl_token::id(),
            associated_token_program: spl_associated_token_account::id(),
            token_0_vault: get_raydium_vault(&raydium_cpmm::id(), &spl_token::native_mint::id()),
            token_1_vault: get_raydium_vault(&raydium_cpmm::id(), &luxor_swap::luxor_mint::id()),
            token_0_account: spl_associated_token_account::get_associated_token_address(
                &program.payer(),
                &spl_token::native_mint::id(),
            ),
            token_1_account: spl_associated_token_account::get_associated_token_address(
                &program.payer(),
                &luxor_swap::luxor_mint::id(),
            ),
            vault_0_mint: spl_token::native_mint::id(),
            vault_1_mint: luxor_swap::luxor_mint::id(),
            raydium_authority: vault_and_lp_mint_auth::id(),
            raydium_cpmm_program: raydium_cpmm::id(),
            amm_config: get_amm_config_address(&raydium_cpmm::id(), 0),
            observation_state: get_observation_state_address(&raydium_cpmm::id()),
        })
        .args(raydium_cp_instructions::Buyback {})
        .instructions()?; // build the instruction(s)

    Ok(ixs)
}

pub fn emergency_withdraw_instr(
    config: &ClientConfig,
    param: u8,
    value: u64,
) -> anyhow::Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.luxor_swap_program)?;

    let ixs = program
        .request()
        .accounts(raydium_cp_accounts::EmergencyWithdraw {
            owner: program.payer(),
            global_config: get_global_config_address(&program.id()),
            authority: get_authority_address(&program.id()),
            luxor_vault_any: get_luxor_vault_address(&program.id()),
            luxor_reward_vault: get_luxor_reward_vault_address(&program.id()),
            admin_stake_info: get_admin_stake_info_address(&program.id()),
            stake_info: get_stake_info_address(&program.id()),
            luxor_mint: luxor_swap::luxor_mint::id(),
            native_mint: spl_token::native_mint::id(),
            sol_treasury_vault: get_sol_treasury_address(&program.id()),
            owner_lxr_token: spl_associated_token_account::get_associated_token_address(
                &program.payer(),
                &luxor_swap::luxor_mint::id(),
            ),
            owner_wsol_token: spl_associated_token_account::get_associated_token_address(
                &program.payer(),
                &spl_token::native_mint::id(),
            ),
            stake_pda: get_stake_pda_address(&program.id()),
            clock: solana_sdk::sysvar::clock::id(),
            token_program: spl_token::id(),
            system_program: system_program::id(),
            associated_token_program: spl_associated_token_account::id(),
        })
        .args(raydium_cp_instructions::EmergencyWithdraw { param, value })
        .instructions()?; // build the instruction(s)

    Ok(ixs)
}

pub fn blacklist_user_instr(
    config: &ClientConfig,
    user: Pubkey,
) -> anyhow::Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.luxor_swap_program)?;

    let ixs = program
        .request()
        .accounts(raydium_cp_accounts::Blacklist {
            owner: program.payer(),
            user,
            user_stake_info: get_user_stake_info_address(&user, &program.id()),
            stake_info: get_stake_info_address(&program.id()),
            admin_stake_info: get_admin_stake_info_address(&program.id()),
            global_config: get_global_config_address(&program.id()),
            system_program: system_program::id(),
        })
        .args(raydium_cp_instructions::Blacklist {})
        .instructions()?; // build the instruction(s)

    Ok(ixs)
}
