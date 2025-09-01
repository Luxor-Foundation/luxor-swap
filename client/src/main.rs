#![allow(dead_code)]
use anchor_client::{Client, Cluster};
use anyhow::{format_err, Result};
use clap::Parser;
use configparser::ini::Ini;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::rc::Rc;
use std::str::FromStr;

mod instructions;
use instructions::amm_instructions::*;
use instructions::rpc::*;

#[derive(Clone, Debug, PartialEq)]
pub struct ClientConfig {
    http_url: String,
    ws_url: String,
    payer_path: String,
    admin_path: String,
    luxor_swap_program: Pubkey,
}

fn load_cfg(client_config: &String) -> Result<ClientConfig> {
    let mut config = Ini::new();
    let _map = config.load(client_config).unwrap();
    let http_url = config.get("Global", "http_url").unwrap();
    if http_url.is_empty() {
        panic!("http_url must not be empty");
    }
    let ws_url = config.get("Global", "ws_url").unwrap();
    if ws_url.is_empty() {
        panic!("ws_url must not be empty");
    }
    let payer_path = config.get("Global", "payer_path").unwrap();
    if payer_path.is_empty() {
        panic!("payer_path must not be empty");
    }
    let admin_path = config.get("Global", "admin_path").unwrap();
    if admin_path.is_empty() {
        panic!("admin_path must not be empty");
    }

    let luxor_swap_program_str = config.get("Global", "luxor_swap_program").unwrap();
    if luxor_swap_program_str.is_empty() {
        panic!("raydium_cp_program must not be empty");
    }
    let luxor_swap_program = Pubkey::from_str(&luxor_swap_program_str).unwrap();

    Ok(ClientConfig {
        http_url,
        ws_url,
        payer_path,
        admin_path,
        luxor_swap_program,
    })
}

fn read_keypair_file(s: &str) -> Result<Keypair> {
    solana_sdk::signature::read_keypair_file(s)
        .map_err(|_| format_err!("failed to read keypair from {}", s))
}

#[derive(Debug, Parser)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: RaydiumCpCommands,
}

#[derive(Debug, Parser)]
pub enum RaydiumCpCommands {
    InitialiseConfigs {
        #[arg(long)]
        admin: Pubkey,
        #[arg(long)]
        vote_account: Pubkey,
        #[arg(long)]
        bonus_rate: u64,
        #[arg(long)]
        max_stake_count_to_get_bonus: u64,
        #[arg(long)]
        min_swap_amount: u64,
        #[arg(long)]
        max_swap_amoumnt: u64,
        #[arg(long)]
        fee_treasury_rate: u64,
        #[arg(long)]
        purchase_enabled: bool,
        #[arg(long)]
        redeem_enabled: bool,
        #[arg(long)]
        initial_lxr_allocation_vault: u64,
    },
    UpdateConfig {
        #[arg(long)]
        param: u8,
        #[arg(long)]
        value: u64,
        #[arg(long)]
        admin: Option<Pubkey>,
    },
    ManualPurchase {
        #[arg(long)]
        user: Pubkey,
        #[arg(long)]
        lxr_purchased: u64,
        #[arg(long)]
        sol_spent: u64,
        #[arg(long)]
        vote_account: Pubkey,
    },
    Purchase {
        #[arg(long)]
        lxr_to_purchase: u64,
        #[arg(long)]
        max_sol_amount: u64,
        #[arg(long)]
        vote_account: Pubkey,
    },
    Redeem {},
    Buyback {},
    EmergencyWithdraw {
        #[arg(long)]
        param: u8,
        #[arg(long)]
        value: u64,
    },
}

fn main() -> Result<()> {
    let client_config = "client_config.ini";
    let pool_config = load_cfg(&client_config.to_string()).unwrap();
    // cluster params.
    let payer = read_keypair_file(&pool_config.payer_path)?;
    // solana rpc client
    let rpc_client = RpcClient::new(pool_config.http_url.to_string());

    // anchor client.
    let anchor_config = pool_config.clone();
    let url = Cluster::Custom(anchor_config.http_url, anchor_config.ws_url);
    let wallet = read_keypair_file(&pool_config.payer_path)?;
    let anchor_client = Client::new(url, Rc::new(wallet));
    let program = anchor_client.program(pool_config.luxor_swap_program)?;

    let opts = Opts::parse();
    match opts.command {
        RaydiumCpCommands::InitialiseConfigs {
            admin,
            vote_account,
            bonus_rate,
            max_stake_count_to_get_bonus,
            min_swap_amount,
            max_swap_amoumnt,
            fee_treasury_rate,
            purchase_enabled,
            redeem_enabled,
            initial_lxr_allocation_vault,
        } => {
            let mut instructions = Vec::new();
            let initialise_ix = initialise_configs_instr(
                &pool_config,
                admin,
                vote_account,
                bonus_rate,
                max_stake_count_to_get_bonus,
                min_swap_amount,
                max_swap_amoumnt,
                fee_treasury_rate,
                purchase_enabled,
                redeem_enabled,
                initial_lxr_allocation_vault,
            )?;
            instructions.extend(initialise_ix);
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &instructions,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("{}", signature);
        }
        RaydiumCpCommands::UpdateConfig {
            param,
            value,
            admin,
        } => {
            let mut instructions = Vec::new();
            let update_config_ix = update_config_instr(&pool_config, param, value, admin)?;
            instructions.extend(update_config_ix);
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &instructions,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("{}", signature);
        }
        RaydiumCpCommands::ManualPurchase {
            user,
            lxr_purchased,
            sol_spent,
            vote_account,
        } => {
            let mut instructions = Vec::new();
            let manual_purchase_ix =
                manual_purchase_instr(&pool_config, user, lxr_purchased, sol_spent, vote_account)?;
            instructions.extend(manual_purchase_ix);
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &instructions,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("{}", signature);
        }
        RaydiumCpCommands::Purchase {
            lxr_to_purchase,
            max_sol_amount,
            vote_account,
        } => {
            let mut instructions = Vec::new();
            let purchase_ix =
                purchase_instr(&pool_config, lxr_to_purchase, max_sol_amount, vote_account)?;
            instructions.extend(purchase_ix);
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &instructions,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("{}", signature);
        }
        RaydiumCpCommands::Redeem {} => {
            let mut instructions = Vec::new();
            let redeem_ix = redeem_instr(&pool_config)?;
            instructions.extend(redeem_ix);
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &instructions,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("{}", signature);
        }
        RaydiumCpCommands::Buyback {} => {
            let mut instructions = Vec::new();
            let buyback_ix = buyback_instr(&pool_config)?;
            instructions.extend(buyback_ix);
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &instructions,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("{}", signature);
        }
        RaydiumCpCommands::EmergencyWithdraw { param, value } => {
            let mut instructions = Vec::new();
            let emergency_withdraw_ix = emergency_withdraw_instr(&pool_config, param, value)?;
            instructions.extend(emergency_withdraw_ix);
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &instructions,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("{}", signature);
        }
    }
    Ok(())
}
