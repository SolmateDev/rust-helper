use anyhow::{anyhow, Result};
use serde_json;
use rand::rngs::OsRng;
use solana_sdk::bs58;
use std::sync::Arc;
use futures::lock::Mutex;
use solana_client::{
    rpc_request::RpcRequest, 
    rpc_response::{
        RpcSimulateTransactionResult
    }, 
    nonblocking::rpc_client::RpcClient, 
    rpc_config::RpcSendTransactionConfig,
};
use solana_sdk::{
    commitment_config::CommitmentConfig, 
    instruction::Instruction,
    program_pack::Pack as TokenPack, pubkey::Pubkey, 
    signature::{Keypair, Signature, Signer}, 
    hash::Hash, transaction::Transaction
};
use spl_token::instruction::{self as token_instruction};
use std::convert::Into;


pub async fn create_account_rent_exempt(
    r_client: Arc<Mutex<RpcClient>>,
    payer: &Keypair,
    data_size: usize,
    owner: &Pubkey,
) -> Result<Keypair> {
    let account = Keypair::generate(&mut OsRng);

    let signers = [payer, &account];
    let lamports;
    let client = r_client.lock().await;
    let x = client.get_minimum_balance_for_rent_exemption(data_size);
    //drop(client);
    lamports = x.await?;

    let create_account_instr = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &account.pubkey(),
        lamports,
        data_size as u64,
        owner,
    );


    let instructions = vec![create_account_instr];


    let recent_hash=get_latest_blockhash(r_client.clone()).await?;
   

    let txn = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &signers,
        recent_hash,
    );

    send_txn(r_client.clone(), &txn, false).await?;
    Ok(account)
}

pub async fn create_token_account(
    r_client: Arc<Mutex<RpcClient>>,
    mint_pubkey: &Pubkey,
    owner_pubkey: &Pubkey,
    payer: &Keypair,
) -> Result<Keypair> {
    let spl_account = Keypair::generate(&mut OsRng);
    let instructions= create_token_account_instructions(
        r_client.clone(),
        spl_account.pubkey(),
        mint_pubkey,
        owner_pubkey,
        payer,
    ).await?;
    
    
    let recent_hash = get_latest_blockhash(r_client.clone()).await?;
    
    let signers = vec![payer, &spl_account];

    let txn = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &signers,
        recent_hash,
    );
    send_txn(r_client.clone(), &txn, false).await?;
    Ok(spl_account)
}

pub async fn create_token_account_instructions(
    r_client: Arc<Mutex<RpcClient>>,
    spl_account: Pubkey,
    mint_pubkey: &Pubkey,
    owner_pubkey: &Pubkey,
    payer: &Keypair,
) -> Result<Vec<Instruction>> {
    let client = r_client.lock().await;
    let x = client.get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);
    //drop(client);
    let lamports = x.await?;

    let create_account_instr = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &spl_account,
        lamports,
        spl_token::state::Account::LEN as u64,
        &spl_token::ID,
    );

    let init_account_instr = token_instruction::initialize_account(
        &spl_token::ID,
        &spl_account,
        &mint_pubkey,
        &owner_pubkey,
    )?;

    let instructions = vec![create_account_instr, init_account_instr];

    Ok(instructions)
}

pub async fn new_mint(
    r_client: Arc<Mutex<RpcClient>>,
    payer_keypair: &Keypair,
    owner_pubkey: &Pubkey,
    decimals: u8,
) -> Result<(Keypair, Signature)> {
    let mint = Keypair::generate(&mut OsRng);
    let s = create_and_init_mint(Arc::clone(&r_client), payer_keypair, &mint, owner_pubkey, decimals).await?;
    Ok((mint, s))
}

pub async fn create_and_init_mint(
    r_client: Arc<Mutex<RpcClient>>,
    payer_keypair: &Keypair,
    mint_keypair: &Keypair,
    owner_pubkey: &Pubkey,
    decimals: u8,
) -> Result<Signature> {
    let signers = vec![payer_keypair, mint_keypair];
    let client = r_client.lock().await;
    let x = client.get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN);
    //drop(client);
    let lamports = x.await?;
    
    
    let create_mint_account_instruction = solana_sdk::system_instruction::create_account(
        &payer_keypair.pubkey(),
        &mint_keypair.pubkey(),
        lamports,
        spl_token::state::Mint::LEN as u64,
        &spl_token::ID,
    );
    let initialize_mint_instruction = token_instruction::initialize_mint(
        &spl_token::ID,
        &mint_keypair.pubkey(),
        owner_pubkey,
        None,
        decimals,
    )?;
    let instructions = vec![create_mint_account_instruction, initialize_mint_instruction];

    let recent_hash = get_latest_blockhash(Arc::clone(&r_client)).await?;
    
    
    let txn = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer_keypair.pubkey()),
        &signers,
        recent_hash,
    );

    return send_txn(Arc::clone(&r_client), &txn, false).await;
}

pub async fn get_minimum_balance_for_rent_exemption(r_client: Arc<Mutex<RpcClient>>, len: usize)->Result<u64>{
    let client = r_client.lock().await;
    let x = client.get_minimum_balance_for_rent_exemption(len);
    //drop(client);
    let ans=x.await?;
    return Ok(ans);
}

pub async fn get_latest_blockhash(r_client: Arc<Mutex<RpcClient>>)->Result<Hash>{
    let client = r_client.lock().await;
    let x= client.get_latest_blockhash();
   // drop(client);
    let recent_hash=x.await?;
    return Ok(recent_hash);
}

pub async fn mint_to_new_account(
    r_client: Arc<Mutex<RpcClient>>,
    payer: &Keypair,
    minting_key: &Keypair,
    mint: &Pubkey,
    quantity: u64,
) -> Result<Keypair> {
    
    let r_client_1 = r_client.clone();

    let recip_keypair = Keypair::generate(&mut OsRng);

    let lamports;
    {
        let client = r_client.lock().await;
        let x = client.get_minimum_balance_for_rent_exemption(spl_token::state::Account::LEN);
        //drop(client);
        lamports = x.await?;
    }
    
    

    let signers = vec![payer, minting_key, &recip_keypair];

    let create_recip_instr = solana_sdk::system_instruction::create_account(
        &payer.pubkey(),
        &recip_keypair.pubkey(),
        lamports,
        spl_token::state::Account::LEN as u64,
        &spl_token::ID,
    );

    let init_recip_instr = token_instruction::initialize_account(
        &spl_token::ID,
        &recip_keypair.pubkey(),
        mint,
        &payer.pubkey(),
    )?;

    let mint_tokens_instr = token_instruction::mint_to(
        &spl_token::ID,
        mint,
        &recip_keypair.pubkey(),
        &minting_key.pubkey(),
        &[],
        quantity,
    )?;

    let instructions = vec![create_recip_instr, init_recip_instr, mint_tokens_instr];

    let recent_hash = get_latest_blockhash(r_client.clone()).await?;

    let txn = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        &signers,
        recent_hash,
    );

    send_txn(r_client_1, &txn, false).await?;
    
    return Ok(recip_keypair);
}

pub async fn transfer(
    r_client: Arc<Mutex<RpcClient>>,
    from: &Pubkey,
    to: &Pubkey,
    amount: u64,
    from_authority: &Keypair,
    payer: &Keypair,
) -> Result<Signature> {
    let instr = token_instruction::transfer(
        &spl_token::ID,
        from,
        to,
        &from_authority.pubkey(),
        &[],
        amount,
    )?;
    let r_1 = r_client.clone();
    let r_2 = r_client.clone();
    let recent_hash = get_latest_blockhash(r_1).await?;
    let signers = [payer, from_authority];
    let txn =
        Transaction::new_signed_with_payer(&[instr], Some(&payer.pubkey()), &signers, recent_hash);
    return send_txn(r_2, &txn, false).await;
}

pub async fn send_txn(r_client: Arc<Mutex<RpcClient>>, txn: &Transaction, _simulate: bool) -> Result<Signature> {
    let client = r_client.lock().await;
    let x= client.send_and_confirm_transaction_with_spinner_and_config(
        txn,
        CommitmentConfig::confirmed(),
        RpcSendTransactionConfig {
            skip_preflight: true,
            ..RpcSendTransactionConfig::default()
        },
    );
    //drop(client);
    return Ok(x.await?);
}

pub async fn simulate_transaction(
    r_client: Arc<Mutex<RpcClient>>,
    transaction: &Transaction,
    sig_verify: bool,
    cfg: CommitmentConfig,
) -> Result<RpcSimulateTransactionResult> {
    let serialized_encoded = bs58::encode(bincode::serialize(transaction).unwrap()).into_string();
    let client = r_client.lock().await;
    let x = client.send(
        RpcRequest::SimulateTransaction,
        serde_json::json!([serialized_encoded, {
            "sigVerify": sig_verify, "commitment": cfg.commitment
        }]),
    );
    //drop(client);
    let result=x.await?;
    return Ok(result);
    
}

pub async fn get_token_account<T: TokenPack>(r_client: Arc<Mutex<RpcClient>>, addr: &Pubkey) -> Result<T> {
    
    let  client = r_client.lock().await;
    let x= client.get_account_with_commitment(addr, CommitmentConfig::processed());
    //drop(client);
    let account = x.await?.value.map_or(Err(anyhow!("Account not found")), Ok)?;
    
    T::unpack_from_slice(&account.data).map_err(Into::into)
}
