use std::sync::Arc;
use bincode::serialize;
use futures::lock::Mutex;
use solana_transaction_status::UiTransactionEncoding;
use tonic::{Response as TonicResponse, Status as TonicStatus, Request};
use crate::rpcnb;
use ed25519_dalek;
use rand::rngs::OsRng;
use spl_token::{
    instruction as token_instruction,
    state as spl_state
};

use solana_client::{nonblocking::rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    transaction::{Transaction, uses_durable_nonce},
    pubkey::{
        Pubkey,
    },
    signer::{keypair::{
        Keypair,

    }, Signer}, program_pack::Pack, signature::Signature, commitment_config::CommitmentConfig
};

use anyhow::{anyhow, Result};

const HEADER_JWT:&str="JWT";

// retrieve the jwt from an incoming gRPC request so that the RPC and WS client can use an authenticated JSON RPC endpoint.
pub fn get_jwt<'a, T>(req: &'a Request<T>)->Option<String>{
    return match req.metadata().get(HEADER_JWT){
        Some(x)=>{
            let ans=match x.to_str(){
                Ok(y)=>Some(String::from(y)),
                Err(e)=>None
            };
            ans
        },
        None=>None
    }
}


pub fn pubkey_from_bytes<'a>(data:&'a Vec<u8>)->Result<Pubkey>{
    let c: &[u8] = &data;
    let mut d: [u8;ed25519_dalek::PUBLIC_KEY_LENGTH]=[0; ed25519_dalek::PUBLIC_KEY_LENGTH];
    let len = data.len();
    if len !=ed25519_dalek::PUBLIC_KEY_LENGTH{
        return Err(anyhow!("pubkey of wrong length"))
    }
    for i in 0..ed25519_dalek::PUBLIC_KEY_LENGTH-1{
        d[i]=c[i];
    }
    return Ok(Pubkey::new_from_array(d));
}

pub fn keypair_to_bytes<'a>(key: &'a Keypair)->Result<Vec<u8>>{
    let mut secret=Vec::new();
    let x = key.to_bytes();
    for i in 0..ed25519_dalek::PUBLIC_KEY_LENGTH-1{
        secret.push(x[i]);
    }
    return Ok(secret);
}

pub fn keypair_from_bytes<'a>(data: &'a Vec<u8>)->Result<Keypair>{
    let c: &[u8] = data;
    match Keypair::from_bytes(c){
        Ok(key)=>{
            return Ok(key);
        },
        Err(_error)=>{
            eprintln!("{}",_error);
            return Err(anyhow!("bad keypair"));
        }
    }
}



pub async fn initialize_token_account(r_client: Arc<Mutex<RpcClient>>, mint: &Pubkey, owner: &Keypair) -> Result<Keypair> {
    let recip_keypair = Keypair::generate(&mut OsRng);
    let r_1 = r_client.clone();
    let r_2 = r_client.clone();
    let r_3 = r_client.clone();
    let lamports = rpcnb::get_minimum_balance_for_rent_exemption(r_1, spl_state::Account::LEN).await?;
    
    let create_recip_instr = solana_sdk::system_instruction::create_account(
        &owner.pubkey(),
        &recip_keypair.pubkey(),
        lamports,
        spl_state::Account::LEN as u64,
        &spl_token::ID,
    );
    let m_1 = mint.clone();
    let init_recip_instr = token_instruction::initialize_account(
        &spl_token::ID,
        &recip_keypair.pubkey(),
        &m_1,
        &owner.pubkey(),
    )?;
    let o_1 = Keypair::from_bytes(owner.to_bytes().as_ref()).unwrap();
    let signers = vec![&o_1, &recip_keypair];
    let instructions = vec![create_recip_instr, init_recip_instr];
    let recent_hash = rpcnb::get_latest_blockhash(r_2).await?;
    let txn = Transaction::new_signed_with_payer(
        &instructions,
        Some(&owner.pubkey()),
        &signers,
        recent_hash,
    );
    send_txn(r_3, &txn, false).await?;
    Ok(recip_keypair)
}



pub async fn mint_to_existing_account(
    r_client: Arc<Mutex<RpcClient>>,
    payer: &Keypair,
    minting_key: &Keypair,
    mint: &Pubkey,
    recipient: &Pubkey,
    quantity: u64,
) -> Result<()> {
    let payer_1 = Keypair::from_bytes(payer.to_bytes().as_ref()).unwrap();
    let minting_key_1 = Keypair::from_bytes(minting_key.to_bytes().as_ref()).unwrap();
    let signers = vec![&payer_1, &minting_key_1];

    let mint_2 = mint.clone();
    let recipient_2 = recipient.clone();
    let mint_tokens_instr = token_instruction::mint_to(
        &spl_token::ID,
        &mint_2,
        &recipient_2,
        &minting_key.pubkey(),
        &[],
        quantity,
    )?;

    let instructions = vec![mint_tokens_instr];
    let recent_hash= rpcnb::get_latest_blockhash(r_client.clone()).await?;
    
    let txn = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &signers,
        recent_hash,
    );
    send_txn(r_client.clone(), &txn, false).await?;
    Ok(())
}


pub async fn create_account(
    r_client: Arc<Mutex<RpcClient>>,
    mint_pubkey: &Pubkey,
    owner_pubkey: &Pubkey,
    payer: &Keypair,
) -> Result<Keypair> {
    let spl_account = Keypair::generate(&mut OsRng);
    let signers = vec![payer, &spl_account];

    let lamports = rpcnb::get_minimum_balance_for_rent_exemption(r_client.clone(),spl_token::state::Account::LEN).await?;
    

    let create_account_instr = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &spl_account.pubkey(),
        lamports,
        spl_token::state::Account::LEN as u64,
        &spl_token::ID,
    );

    let init_account_instr = token_instruction::initialize_account(
        &spl_token::ID,
        &spl_account.pubkey(),
        &mint_pubkey,
        &owner_pubkey,
    )?;

    let instructions = vec![create_account_instr, init_account_instr];

    let recent_hash = rpcnb::get_latest_blockhash(r_client.clone()).await?;

    let txn = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &signers,
        recent_hash,
    );

    println!("Creating account: {} ...", spl_account.pubkey());
    rpcnb::send_txn(r_client.clone(), &txn, false).await?;
    Ok(spl_account)
}


pub async fn send_txn(r_client: Arc<Mutex<RpcClient>>, txn: &Transaction, _simulate: bool) -> Result<Signature> {
    let sig;
    {
        let client = r_client.lock().await;   
        let x = client.send_and_confirm_transaction_with_spinner_and_config(
            txn,
            CommitmentConfig::confirmed(),
            RpcSendTransactionConfig {
                skip_preflight: true,
                ..RpcSendTransactionConfig::default()
            },
        );
       // drop(client);
        sig = x.await?;
    }
    {
        let client = r_client.lock().await;
        let x = client.confirm_transaction(&sig);
        //drop(client);
        if x.await?{
            return Ok(sig);
        } else {
            return Err(anyhow!("no signature"));
        }
    }
}