#![deny(unaligned_references)]
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use bincode::serialize;
use std::borrow::Cow;
use std::cmp::{max, min};
use std::collections::BTreeSet;
use std::convert::identity;
use std::mem::size_of;

use std::sync::{Arc};
use futures::lock::Mutex;
use std::{thread, time};

use futures::future::join_all;


use anyhow::{format_err};

use debug_print::debug_println;
use log::{error, info};
use rand::rngs::OsRng;
use safe_transmute::{
    guard::SingleManyGuard,
    to_bytes::{transmute_one_to_bytes, transmute_to_bytes},
    transmute_many, transmute_many_pedantic, transmute_one_pedantic,
};
use sloggers::file::FileLoggerBuilder;
use sloggers::types::Severity;
use sloggers::Build;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_sdk::signature::{Keypair, Signer};
use solana_sdk::transaction::Transaction;
use slog_scope;


use serum_common::client::Cluster;
use serum_dex::instruction::{
    cancel_order_by_client_order_id as cancel_order_by_client_order_id_ix,
    close_open_orders as close_open_orders_ix, init_open_orders as init_open_orders_ix,
    MarketInstruction, NewOrderInstructionV3,
};

use serum_dex::state::gen_vault_signer_key;
use serum_dex::state::Event;
use serum_dex::state::EventQueueHeader;
use serum_dex::state::QueueHeader;
use serum_dex::state::Request;
use serum_dex::state::RequestQueueHeader;
use serum_dex::state::{AccountFlag, Market, MarketState, MarketStateV2};
use solmate_rust_helper::{rpcnb, convert_pubkey};
use solmate_rust_helper::serum::{MarketPubkeys as PbMarketPubkeys};

use super::reverse_market_pubkeys;



pub fn with_logging<F: FnOnce()>(_to: &str, fnc: F) {
    fnc();
}

fn read_keypair_file(s: &str) -> Result<Keypair> {
    solana_sdk::signature::read_keypair_file(s)
        .map_err(|_| format_err!("failed to read keypair from {}", s))
}

pub struct Opts {
    pub cluster: Cluster,
    pub command: Command,
}

impl Opts {
    fn client(&self) -> RpcClient {
        RpcClient::new(self.cluster.url().to_string())
    }
}



pub enum Command {
    Genesis {
        payer: String,
        mint: String,
        owner_pubkey: Pubkey,
        decimals: u8,
    },
    Mint {
        payer: String,        
        signer: String,
        mint_pubkey: Pubkey,
        recipient: Option<Pubkey>,
        quantity: u64,
    },
    CreateAccount {
        mint_pubkey: Pubkey,
        owner_pubkey: Pubkey,
        payer: String,
    },
    ConsumeEvents {
        
        dex_program_id: Pubkey,
        payer: String,
        market: Pubkey,
        coin_wallet: Pubkey,
        pc_wallet: Pubkey,
        num_workers: usize,
        events_per_worker: usize,
        num_accounts: Option<usize>,
        log_directory: String,
        max_q_length: Option<u64>,
        max_wait_for_events_delay: Option<u64>,
    },
    MatchOrders {
        dex_program_id: Pubkey,
        payer: String,
        market: Pubkey,        
        coin_wallet: Pubkey,
        pc_wallet: Pubkey,
    },
    MonitorQueue {
        dex_program_id: Pubkey,
        market: Pubkey,
        port: u16,
    },
    PrintEventQueue {
        dex_program_id: Pubkey,
        market: Pubkey,
    },
    WholeShebang {
        payer: String,
        dex_program_id: Pubkey,
    },
    SettleFunds {
        payer: String,
        dex_program_id: Pubkey,
        market: Pubkey,
        orders: Pubkey,
        coin_wallet: Pubkey,
        pc_wallet: Pubkey,
        signer: Option<String>,
    },
    ListMarket {
        payer: String,
        dex_program_id: Pubkey,
        coin_mint: Pubkey,
        pc_mint: Pubkey,
        coin_lot_size: Option<u64>,
        pc_lot_size: Option<u64>,
    },
    InitializeTokenAccount {
        mint: Pubkey,
        owner_account: String,
    },
}


#[derive(Debug)]
pub struct MarketPubkeys {
    pub market: Box<Pubkey>,
    pub req_q: Box<Pubkey>,
    pub event_q: Box<Pubkey>,
    pub bids: Box<Pubkey>,
    pub asks: Box<Pubkey>,
    pub coin_vault: Box<Pubkey>,
    pub pc_vault: Box<Pubkey>,
    pub vault_signer_key: Box<Pubkey>,
}

pub struct SerumClient<'b>{
    rpc: Arc<Mutex<RpcClient>>,
    jwt: &'b Option<String>
}

impl<'b> SerumClient <'b>{
    pub fn new<'a:'b>(r_client: Arc<Mutex<RpcClient>>,jwt: &'a Option<String>)->Self{
        let rpc=r_client.clone();
        return Self{
            rpc,jwt: &jwt
        }
    }

    #[cfg(target_endian = "little")]
    pub async fn get_keys_for_market<'a: 'b>(
        &'b self,
        program_id: &'a Pubkey,
        market: &'a Pubkey,
    ) -> Result<MarketPubkeys> {
        let f_client = self.rpc.clone();
        let client = f_client.lock().await;
        let x = client.get_account_data(&market);
        let account_data;
        //drop(client);
        match x.await {
            Ok(y)=>{
                account_data=y;
            },
            Err(e)=>{
                return Err(anyhow!("{}",e));
            }
        }
        

        let words: Cow<[u64]> = remove_dex_account_padding(&account_data)?;
        let market_state: MarketState = {
            let account_flags = Market::account_flags(&account_data)?;
            if account_flags.intersects(AccountFlag::Permissioned) {
                let state = transmute_one_pedantic::<MarketStateV2>(transmute_to_bytes(&words))
                    .map_err(|e| e.without_src())?;
                state.check_flags(true)?;
                state.inner
            } else {
                let state = transmute_one_pedantic::<MarketState>(transmute_to_bytes(&words))
                    .map_err(|e| e.without_src())?;
                state.check_flags(true)?;
                state
            }
        };
        let vault_signer_key =
            gen_vault_signer_key(market_state.vault_signer_nonce, market, program_id)?;
        assert_eq!(
            transmute_to_bytes(&identity(market_state.own_address)),
            market.as_ref()
        );
        Ok(MarketPubkeys {
            market: Box::new(*market),
            req_q: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
                market_state.req_q,
            )))),
            event_q: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
                market_state.event_q,
            )))),
            bids: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
                market_state.bids,
            )))),
            asks: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
                market_state.asks,
            )))),
            coin_vault: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
                market_state.coin_vault,
            )))),
            pc_vault: Box::new(Pubkey::new(transmute_one_to_bytes(&identity(
                market_state.pc_vault,
            )))),
            vault_signer_key: Box::new(vault_signer_key),
        })
    }


    #[cfg(target_endian = "little")]
    pub async fn consume_events<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        payer: &'a Keypair,
        state: &'a MarketPubkeys,
        coin_wallet: &'a Pubkey,
        pc_wallet: &'a Pubkey,
    ) -> Result<Vec<u8>> {
        
        let i= self.consume_events_instruction(program_id, state, coin_wallet, pc_wallet).await?;
        

        if i.is_none(){
            return Err(anyhow!("no instruction"));
        }
        let instruction=i.unwrap();
        
        let recent_hash=rpcnb::get_latest_blockhash(self.rpc.clone()).await?;
        
        
        
        info!("Consuming events ...");
        let txn = Transaction::new_signed_with_payer(
            std::slice::from_ref(&instruction),
            Some(&payer.pubkey()),
            &[payer],
            recent_hash,
        );
        info!("Consuming events ...");
        //if util::send_txn(r_client.clone(), &txn, false).await.is_err(){
        //    return Err(anyhow!("failed to send tx"));
        //}
        let data = serialize::<Transaction>(&txn)?;
        //return Ok(());
        return Ok(data);
    }


    pub async fn consume_events_wrapper<'a:'b>(
        &'b self,
        program_id: Pubkey,
        payer: Keypair,
        account_metas: Vec<AccountMeta>,
        thread_num: usize,
        to_consume: usize,
        event_q: Pubkey,
        max_slot_height_mutex: Arc<Mutex<u64>>,
        slot: u64,
    ) {
        let start = std::time::Instant::now();
        let result = self.consume_events_once(
            program_id,
            payer,
            account_metas,
            to_consume,
            thread_num,
            event_q,
        ).await;
        match result {
            Ok(signature) => {
                info!(
                    "[thread {}] Successfully consumed events after {:?}: {}.",
                    thread_num,
                    start.elapsed(),
                    signature
                );
                let mut max_slot_height = max_slot_height_mutex.lock().await;
                *max_slot_height = max(slot, *max_slot_height);
            }
            Err(err) => {
                error!("[thread {}] Received error: {:?}", thread_num, err);
            }
        };
    }

    pub async fn consume_events_once<'a:'b>(
        &'b self,
        program_id: Pubkey,
        payer: Keypair,
        account_metas: Vec<AccountMeta>,
        to_consume: usize,
        _thread_number: usize,
        event_q: Pubkey,
    ) -> Result<Signature> {
        let _start = std::time::Instant::now();
        let instruction_data: Vec<u8> = MarketInstruction::ConsumeEvents(to_consume as u16).pack();
        let instruction = Instruction {
            program_id,
            accounts: account_metas,
            data: instruction_data,
        };
        let random_instruction = solana_sdk::system_instruction::transfer(
            &payer.pubkey(),
            &payer.pubkey(),
            rand::random::<u64>() % 10000 + 1,
        );
        
        let recent_hash=rpcnb::get_latest_blockhash(self.rpc.clone()).await?;
        
        let txn = Transaction::new_signed_with_payer(
            &[instruction, random_instruction],
            Some(&payer.pubkey()),
            &[&payer],
            recent_hash,
        );
    
        info!("Consuming events ...");
        
        let signature;
        {
            let f_client = self.rpc.clone();
            let client =f_client.lock().await;
            let x = client.send_transaction_with_config(
                &txn,
                RpcSendTransactionConfig {
                    skip_preflight: true,
                    ..RpcSendTransactionConfig::default()
                },
            );
            //drop(client);
            match x.await {
                Ok(y)=>{
                    signature = y;
                },
                Err(e)=>{
                    return Err(anyhow!("{}",e));
                }
            }
        }
        
        return Ok(signature);
    }

    pub async fn consume_events_instruction<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        state: &'a MarketPubkeys,
        coin_wallet: &'a Pubkey,
        pc_wallet: &'a Pubkey,
    ) -> Result<Option<Instruction>> {
    
    
        let event_q_data;
        {
            let r_client = self.rpc.clone();
            let client = r_client.lock().await;
            let x = client.get_account_data(&state.event_q);
            //drop(client);
            event_q_data=x.await?; 
        }
        
        let inner: Cow<[u64]> = remove_dex_account_padding(&event_q_data)?;
        let (_header, seg0, seg1) = parse_event_queue(&inner)?;
    
        if seg0.len() + seg1.len() == 0 {
            info!("Total event queue length: 0, returning early");
            return Ok(None);
        } else {
            info!("Total event queue length: {}", seg0.len() + seg1.len());
        }
        let accounts = seg0.iter().chain(seg1.iter()).map(|event| event.owner);
        let mut orders_accounts: Vec<_> = accounts.collect();
        orders_accounts.sort_unstable();
        orders_accounts.dedup();
        // todo: Shuffle the accounts before truncating, to avoid favoring low sort order accounts
        orders_accounts.truncate(32);
        info!("Number of unique order accounts: {}", orders_accounts.len());
    
        let mut account_metas = Vec::with_capacity(orders_accounts.len() + 4);
        for pubkey_words in orders_accounts {
            let pubkey = Pubkey::new(transmute_to_bytes(&pubkey_words));
            account_metas.push(AccountMeta::new(pubkey, false));
        }
        for pubkey in [&state.market, &state.event_q, coin_wallet, pc_wallet].iter() {
            account_metas.push(AccountMeta::new(**pubkey, false));
        }
    
        let instruction_data: Vec<u8> =
            MarketInstruction::ConsumeEvents(account_metas.len() as u16).pack();
    
        let instruction = Instruction {
            program_id: *program_id,
            accounts: account_metas,
            data: instruction_data,
        };
    
        Ok(Some(instruction))
    }

    pub async fn cancel_order_by_client_order_id<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        owner: &'a Keypair,
        state: &'a MarketPubkeys,
        orders: &'a Pubkey,
        client_order_id: u64,
    ) -> Result<Vec<u8>> {
        let ixs = &[cancel_order_by_client_order_id_ix(
            program_id,
            &state.market,
            &state.bids,
            &state.asks,
            orders,
            &owner.pubkey(),
            &state.event_q,
            client_order_id,
        )?];
        let recent_hash = rpcnb::get_latest_blockhash(self.rpc.clone()).await?;
        
        let txn = Transaction::new_signed_with_payer(ixs, Some(&owner.pubkey()), &[owner], recent_hash);
    
        debug_println!("Canceling order by client order id instruction ...");
        let result = rpcnb::simulate_transaction(self.rpc.clone(), &txn, true, CommitmentConfig::confirmed()).await?;
        if let Some(e) = result.err {
            //debug_println!("{:#?}", result.value.logs);
            return Err(format_err!("simulate_transaction error: {:?}", e));
        }
    
        let signed_tx = serialize::<Transaction>(&txn)?;
        //rpcnb::send_txn(r_client.clone(), &txn, false).await?;
        return Ok(signed_tx);
    }

    pub async fn close_open_orders<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        owner: &'a Keypair,
        state: &'a MarketPubkeys,
        orders: &'a Pubkey,
    ) -> Result<Vec<u8>> {
        debug_println!("Closing open orders...");
        let ixs = &[close_open_orders_ix(
            program_id,
            orders,
            &owner.pubkey(),
            &owner.pubkey(),
            &state.market,
        )?];
        let recent_hash = rpcnb::get_latest_blockhash(self.rpc.clone()).await?;
    
        let txn = Transaction::new_signed_with_payer(ixs, Some(&owner.pubkey()), &[owner], recent_hash);
    
        debug_println!("Simulating close open orders instruction ...");
        let result = rpcnb::simulate_transaction(self.rpc.clone(), &txn, true, CommitmentConfig::confirmed()).await?;
        if let Some(e) = result.err {
            //debug_println!("{:#?}", result.value.logs);
            return Err(format_err!("simulate_transaction error: {:?}", e));
        }
    
        return match serialize::<Transaction>(&txn){
            Ok(x)=>Ok(x),
            Err(e)=>Err(anyhow!("bad serialization"))
        }
    
        //util::send_txn(r_client.clone(), &txn, false).await?;
        //Ok(())
    }
    
    pub async fn init_open_orders<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        owner: &'a Keypair,
        state: &'a MarketPubkeys,
        orders: &'a mut Option<Pubkey>,
    ) -> Result<Vec<u8>> {
        let mut instructions = Vec::new();
        let orders_keypair;
        let mut signers = Vec::new();
        let orders_pubkey = match *orders {
            Some(pk) => pk,
            None => {
                let (orders_key, instruction) = self.create_dex_account(
                    program_id,
                    &owner.pubkey(),
                    size_of::<serum_dex::state::OpenOrders>(),
                ).await?;
                orders_keypair = orders_key;
                signers.push(&orders_keypair);
                instructions.push(instruction);
                orders_keypair.pubkey()
            }
        };
        *orders = Some(orders_pubkey);
        instructions.push(init_open_orders_ix(
            program_id,
            &orders_pubkey,
            &owner.pubkey(),
            &state.market,
            None,
        )?);
        signers.push(owner);
    

        let recent_hash = rpcnb::get_latest_blockhash(self.rpc.clone()).await?;
        let txn = Transaction::new_signed_with_payer(
            &instructions,
            Some(&owner.pubkey()),
            &signers,
            recent_hash,
        );
        let signed_tx = serialize::<Transaction>(&txn)?;
        return Ok(signed_tx);
        //util::send_txn(r_client.clone(), &txn, false).await?;
        //Ok(())
    }

    pub async fn place_order<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        payer: &'a Keypair,
        wallet: &'a Pubkey,
        state: &'a MarketPubkeys,
        orders: &'a mut Option<Pubkey>,
        new_order: NewOrderInstructionV3,
    ) -> Result<Vec<u8>> {
        let mut instructions = Vec::new();
        let orders_keypair;
        let mut signers = Vec::new();
        let orders_pubkey = match *orders {
            Some(pk) => pk,
            None => {
                let (orders_key, instruction) = self.create_dex_account(
                    program_id,
                    &payer.pubkey(),
                    size_of::<serum_dex::state::OpenOrders>(),
                ).await?;
                orders_keypair = orders_key;
                signers.push(&orders_keypair);
                instructions.push(instruction);
                orders_keypair.pubkey()
            }
        };
        *orders = Some(orders_pubkey);
        let _side = new_order.side;
        let data = MarketInstruction::NewOrderV3(new_order).pack();
        let instruction = Instruction {
            program_id: *program_id,
            data,
            accounts: vec![
                AccountMeta::new(*state.market, false),
                AccountMeta::new(orders_pubkey, false),
                AccountMeta::new(*state.req_q, false),
                AccountMeta::new(*state.event_q, false),
                AccountMeta::new(*state.bids, false),
                AccountMeta::new(*state.asks, false),
                AccountMeta::new(*wallet, false),
                AccountMeta::new_readonly(payer.pubkey(), true),
                AccountMeta::new(*state.coin_vault, false),
                AccountMeta::new(*state.pc_vault, false),
                AccountMeta::new_readonly(spl_token::ID, false),
                AccountMeta::new_readonly(solana_sdk::sysvar::rent::ID, false),
            ],
        };
        instructions.push(instruction);
        signers.push(payer);
    
        let recent_hash = rpcnb::get_latest_blockhash(self.rpc.clone()).await?;
        
        let txn = Transaction::new_signed_with_payer(
            &instructions,
            Some(&payer.pubkey()),
            &signers,
            recent_hash,
        );
        return match serialize::<Transaction>(&txn){
            Ok(x)=>Ok(x),
            Err(e)=>Err(anyhow!("{}",e))
        }
        //return util::send_txn(r_client.clone(), &txn, false).await;
    }

    pub async fn settle_funds<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        payer: &'a Keypair,
        state: &'a MarketPubkeys,
        signer: Option<&'a Keypair>,
        orders: &'a Pubkey,
        coin_wallet: &'a Pubkey,
        pc_wallet: &'a Pubkey,
    ) -> Result<Vec<u8>> {
        let data = MarketInstruction::SettleFunds.pack();
        let instruction = Instruction {
            program_id: *program_id,
            data,
            accounts: vec![
                AccountMeta::new(*state.market, false),
                AccountMeta::new(*orders, false),
                AccountMeta::new_readonly(signer.unwrap_or(payer).pubkey(), true),
                AccountMeta::new(*state.coin_vault, false),
                AccountMeta::new(*state.pc_vault, false),
                AccountMeta::new(*coin_wallet, false),
                AccountMeta::new(*pc_wallet, false),
                AccountMeta::new_readonly(*state.vault_signer_key, false),
                AccountMeta::new_readonly(spl_token::ID, false),
            ],
        };
        let recent_hash = rpcnb::get_latest_blockhash(self.rpc.clone()).await?;
        let mut signers = vec![payer];
        if let Some(s) = signer {
            signers.push(s);
        }
        let txn = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&payer.pubkey()),
            &signers,
            recent_hash,
        );
        let mut i = 0;
        loop {
            i += 1;
            assert!(i < 10);
            debug_println!("Simulating SettleFunds instruction ...");
            let result = rpcnb::simulate_transaction(self.rpc.clone(), &txn, true, CommitmentConfig::processed()).await?;
            
            if let Some(e) = result.err {
                return Err(format_err!("simulate_transaction error: {:?}", e));
            } else {
                break;
            }
        }
        debug_println!("Settling ...");
        return match serialize::<Transaction>(&txn){
            Ok(x)=>Ok(x),
            Err(e)=>Err(anyhow!("{}",e))
        }
        //rpcnb::send_txn(r_client.clone(), &txn, false).await?;
        //Ok(())
    }

    pub async fn list_market<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        payer: &'a Keypair,
        coin_mint: &'a Pubkey,
        pc_mint: &'a Pubkey,
        coin_lot_size: u64,
        pc_lot_size: u64,
    ) -> Result<(Vec<u8>,MarketPubkeys)> {
        let (listing_keys, mut instructions) =
            self.gen_listing_params( program_id, &payer.pubkey(), coin_mint, pc_mint).await?;
        let ListingKeys {
            market_key,
            req_q_key,
            event_q_key,
            bids_key,
            asks_key,
            vault_signer_pk,
            vault_signer_nonce,
        } = listing_keys;
    
        debug_println!("Creating coin vault...");
        let coin_vault = rpcnb::create_token_account(self.rpc.clone(), coin_mint, &vault_signer_pk, payer).await?;
        debug_println!("Created account: {} ...", coin_vault.pubkey());
    
        debug_println!("Creating pc vault...");
        let pc_vault = rpcnb::create_token_account(self.rpc.clone(), pc_mint, &listing_keys.vault_signer_pk, payer).await?;
        debug_println!("Created account: {} ...", pc_vault.pubkey());
    
        let init_market_instruction = serum_dex::instruction::initialize_market(
            &market_key.pubkey(),
            program_id,
            coin_mint,
            pc_mint,
            &coin_vault.pubkey(),
            &pc_vault.pubkey(),
            None,
            None,
            None,
            &bids_key.pubkey(),
            &asks_key.pubkey(),
            &req_q_key.pubkey(),
            &event_q_key.pubkey(),
            coin_lot_size,
            pc_lot_size,
            vault_signer_nonce,
            100,
        )?;
        debug_println!(
            "initialize_market_instruction: {:#?}",
            &init_market_instruction
        );
    
        instructions.push(init_market_instruction);
    
        let recent_hash = rpcnb::get_latest_blockhash(self.rpc.clone()).await?;
        
        let signers = vec![
            payer,
            &market_key,
            &req_q_key,
            &event_q_key,
            &bids_key,
            &asks_key,
            &req_q_key,
            &event_q_key,
        ];
        let txn = Transaction::new_signed_with_payer(
            &instructions,
            Some(&payer.pubkey()),
            &signers,
            recent_hash,
        );
    
        let signed_tx = serialize::<Transaction>(&txn)?;
        
    
        Ok((signed_tx,MarketPubkeys {
            market: Box::new(market_key.pubkey()),
            req_q: Box::new(req_q_key.pubkey()),
            event_q: Box::new(event_q_key.pubkey()),
            bids: Box::new(bids_key.pubkey()),
            asks: Box::new(asks_key.pubkey()),
            coin_vault: Box::new(coin_vault.pubkey()),
            pc_vault: Box::new(pc_vault.pubkey()),
            vault_signer_key: Box::new(vault_signer_pk),
        }))
    }

    pub async fn gen_listing_params<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        payer: &'a Pubkey,
        _coin_mint: &'a Pubkey,
        _pc_mint: &'a Pubkey,
    ) -> Result<(ListingKeys, Vec<Instruction>)> {
        let (market_key, create_market) = self.create_dex_account(program_id, payer, 376).await?;
        let (req_q_key, create_req_q) = self.create_dex_account(program_id, payer, 640).await?;
        let (event_q_key, create_event_q) = self.create_dex_account(program_id, payer, 1 << 20).await?;
        let (bids_key, create_bids) = self.create_dex_account(program_id, payer, 1 << 16).await?;
        let (asks_key, create_asks) = self.create_dex_account(program_id, payer, 1 << 16).await?;
        let (vault_signer_nonce, vault_signer_pk) = {
            let mut i = 0;
            loop {
                assert!(i < 100);
                if let Ok(pk) = gen_vault_signer_key(i, &market_key.pubkey(), program_id) {
                    break (i, pk);
                }
                i += 1;
            }
        };
        let info = ListingKeys {
            market_key,
            req_q_key,
            event_q_key,
            bids_key,
            asks_key,
            vault_signer_pk,
            vault_signer_nonce,
        };
        let instructions = vec![
            create_market,
            create_req_q,
            create_event_q,
            create_bids,
            create_asks,
        ];
        Ok((info, instructions))
    }
    

    pub async fn create_dex_account<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        payer: &'a Pubkey,
        unpadded_len: usize,
    ) -> Result<(Keypair, Instruction)> {
        let len = unpadded_len + 12;
        let key = Keypair::generate(&mut OsRng);
        
        let create_account_instr = solana_sdk::system_instruction::create_account(
            payer,
            &key.pubkey(),
            rpcnb::get_minimum_balance_for_rent_exemption(self.rpc.clone(),len).await?,
            len as u64,
            program_id,
        );
        Ok((key, create_account_instr))
    }

    pub async fn match_orders<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        payer: &'a Keypair,
        state: &'a MarketPubkeys,
        coin_wallet: &'a Pubkey,
        pc_wallet: &'a Pubkey,
    ) -> Result<Vec<u8>> {
        let instruction_data: Vec<u8> = MarketInstruction::MatchOrders(2).pack();
    
        let instruction = Instruction {
            program_id: *program_id,
            accounts: vec![
                AccountMeta::new(*state.market, false),
                AccountMeta::new(*state.req_q, false),
                AccountMeta::new(*state.event_q, false),
                AccountMeta::new(*state.bids, false),
                AccountMeta::new(*state.asks, false),
                AccountMeta::new(*coin_wallet, false),
                AccountMeta::new(*pc_wallet, false),
            ],
            data: instruction_data,
        };
    
        let recent_hash = rpcnb::get_latest_blockhash(self.rpc.clone()).await?;
        let txn = Transaction::new_signed_with_payer(
            std::slice::from_ref(&instruction),
            Some(&payer.pubkey()),
            &[payer],
            recent_hash,
        );
    
        debug_println!("Simulating order matching ...");
        let result = rpcnb::simulate_transaction(self.rpc.clone(), &txn, true, CommitmentConfig::processed()).await?;
        if let Some(e) = result.err {
            return Err(format_err!("simulate_transaction error: {:?}", e));
        } else {
            debug_println!("Matching orders ...");
            //util::send_txn(r_client.clone(), &txn, false).await?;
            return match serialize::<Transaction>(&txn){
                Ok(x)=>Ok(x),
                Err(e)=>Err(anyhow!("{}",e))
            }
            //return Ok(());
        }
        //debug_println!("{:#?}", result.value);
        
    }


    pub async fn convert_state<'a:'b>(
        &'b self,
        program_id: &'a Pubkey,
        req: &'a solmate_rust_helper::serum::MarketState
    )->Result<MarketPubkeys>{
        let state;
        let errmsg="bad market state";
        
        match &req.market{
            Some(solmate_rust_helper::serum::market_state::Market::Id(id))=>{
                let market=convert_pubkey(id).or(Err(anyhow!(errmsg)))?;
                state = self.get_keys_for_market( &program_id, &market).await.or(Err(anyhow!("failed to get market")));
            },
            Some(solmate_rust_helper::serum::market_state::Market::State(pb_state)) => {
                state=reverse_market_pubkeys(pb_state);
            },
            _=>{
                return Err(anyhow!("blank market"));
            }
        }

        return state;
        
    }


    pub async fn read_queue_length_loop_single<'a:'b >(
        &'b self,
        program_id: Pubkey,
        market: &'a Pubkey,
        port: u16,
    ) -> Result<()> {
        
        let market_keys = self.get_keys_for_market(&program_id, market).await?;

        let client = self.rpc.lock().await;
        
        let x = client
                    .get_account_with_commitment(&market_keys.event_q, CommitmentConfig::processed());
        let event_q_data = x.await?.value.ok_or(anyhow!("no value"))?.data;
        let inner = remove_dex_account_padding(event_q_data.as_ref())?;
        let (event_q_header, seg0, seg1) = parse_event_queue(&inner)?;
        let len = seg0.len() + seg1.len();

        return Ok(());
        
    }
}


#[cfg(target_endian = "little")]
pub(crate) fn remove_dex_account_padding<'a>(data: &'a [u8]) -> Result<Cow<'a, [u64]>> {
    use serum_dex::state::{ACCOUNT_HEAD_PADDING, ACCOUNT_TAIL_PADDING};
    let head = &data[..ACCOUNT_HEAD_PADDING.len()];
    if data.len() < ACCOUNT_HEAD_PADDING.len() + ACCOUNT_TAIL_PADDING.len() {
        return Err(format_err!(
            "dex account length {} is too small to contain valid padding",
            data.len()
        ));
    }
    if head != ACCOUNT_HEAD_PADDING {
        return Err(format_err!("dex account head padding mismatch"));
    }
    let tail = &data[data.len() - ACCOUNT_TAIL_PADDING.len()..];
    if tail != ACCOUNT_TAIL_PADDING {
        return Err(format_err!("dex account tail padding mismatch"));
    }
    let inner_data_range = ACCOUNT_HEAD_PADDING.len()..(data.len() - ACCOUNT_TAIL_PADDING.len());
    let inner: &'a [u8] = &data[inner_data_range];
    let words: Cow<'a, [u64]> = match transmute_many_pedantic::<u64>(inner) {
        Ok(word_slice) => Cow::Borrowed(word_slice),
        Err(transmute_error) => {
            let word_vec = transmute_error.copy().map_err(|e| e.without_src())?;
            Cow::Owned(word_vec)
        }
    };
    Ok(words)
}



pub(crate) fn parse_event_queue(data_words: &[u64]) -> Result<(EventQueueHeader, &[Event], &[Event])> {
    let (header_words, event_words) = data_words.split_at(size_of::<EventQueueHeader>() >> 3);
    let header: EventQueueHeader =
        transmute_one_pedantic(transmute_to_bytes(header_words)).map_err(|e| e.without_src())?;
    let events: &[Event] = transmute_many::<_, SingleManyGuard>(transmute_to_bytes(event_words))
        .map_err(|e| e.without_src())?;
    let (tail_seg, head_seg) = events.split_at(header.head() as usize);
    let head_len = head_seg.len().min(header.count() as usize);
    let tail_len = header.count() as usize - head_len;
    Ok((header, &head_seg[..head_len], &tail_seg[..tail_len]))
}

pub(crate) fn parse_req_queue(data_words: &[u64]) -> Result<(RequestQueueHeader, &[Request], &[Request])> {
    let (header_words, request_words) = data_words.split_at(size_of::<RequestQueueHeader>() >> 3);
    let header: RequestQueueHeader =
        transmute_one_pedantic(transmute_to_bytes(header_words)).map_err(|e| e.without_src())?;
    let request: &[Request] =
        transmute_many::<_, SingleManyGuard>(transmute_to_bytes(request_words))
            .map_err(|e| e.without_src())?;
    let (tail_seg, head_seg) = request.split_at(header.head() as usize);
    let head_len = head_seg.len().min(header.count() as usize);
    let tail_len = header.count() as usize - head_len;
    Ok((header, &head_seg[..head_len], &tail_seg[..tail_len]))
}

pub(crate) fn hash_accounts(val: &[u64; 4]) -> u64 {
    val.iter().fold(0, |a, b| b.wrapping_add(a))
}

fn init_logger(log_directory: &str) {
    let path = std::path::Path::new(log_directory);
    let parent = path.parent().unwrap();
    std::fs::create_dir_all(parent).unwrap();
    let mut builder = FileLoggerBuilder::new(log_directory);
    builder.level(Severity::Info).rotate_size(8 * 1024 * 1024);
    let log = builder.build().unwrap();
    let _guard = slog_scope::set_global_logger(log);
    _guard.cancel_reset();
    slog_stdlog::init().unwrap();
}








pub struct ListingKeys {
    market_key: Keypair,
    req_q_key: Keypair,
    event_q_key: Keypair,
    bids_key: Keypair,
    asks_key: Keypair,
    vault_signer_pk: Pubkey,
    vault_signer_nonce: u64,
}








pub(crate) enum MonitorEvent {
    NumEvents(usize),
    NewConn(std::net::TcpStream),
}

