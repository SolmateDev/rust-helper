use bincode::{
    deserialize,
    Result as BincodeResult
};

use std::{
    time::{Duration, Instant},
    thread::sleep,
    sync::{Arc},
    thread
};
use futures::lock::Mutex;
use solana_sdk::{
    pubkey::{
        Pubkey,
    },
    signer::keypair::{
        Keypair,
    },
    signature::Signature,
    clock::MAX_PROCESSING_AGE,
    transaction::Transaction, commitment_config::{CommitmentLevel, CommitmentConfig},
};

use solana_client::{
    nonblocking::rpc_client::RpcClient, 
    tpu_client::{
        TpuClientConfig, TpuClient,
    }, rpc_config::RpcSendTransactionConfig
};
use solmate_rust_helper::{
    rpcnb,
    convert_pubkey,convert_keypair,reverse_keypair, copy_keypair
};
use tonic::{Request, Response, Status};
use solmate_rust_helper::{config,solanabase,util,basic};

use solanabase::{
    broadcast_server::{
        Broadcast, BroadcastServer
    },
    InitializeTokenAccount, SendBatchRequest,SendBatchResponse,
    CreateAccount,Mint,Genesis
};

#[derive()]
pub struct MyServer {
    cluster: config::Cluster,
    rpc: Arc<Mutex<RpcClient>>,
    tpu: Arc<Mutex<TpuClient>>,
}




#[tonic::async_trait]
impl Broadcast for MyServer {
    
    async fn send_tx<'a>(&'a self, msg: Request<SendBatchRequest>)->Result<Response<SendBatchResponse>,Status>{
        let req = msg.get_ref();
        let tx_list = req.tx.clone();

        
        
        let mut sig_list : Vec<SendResult> = Vec::new();  
        for serialized_tx_orig in tx_list.iter() {
            let serialized_tx = serialized_tx_orig.clone();
            let array: &[u8] = &serialized_tx;
            let deserialize_result: BincodeResult<Transaction> = deserialize(array);
            if deserialize_result.is_err() {
                return Err(Status::internal("failed to deserialize"));
            }
            let tx = deserialize_result.unwrap();

            let mut x = vec![SendResult::new(tx,serialized_tx)];
            sig_list.append(&mut x);
        }

        let wait_time = MAX_PROCESSING_AGE;
        let mut confirmed_tx_count = 0;
        let now = Instant::now();

        
        while now.elapsed().as_secs() <= wait_time as u64 && confirmed_tx_count < sig_list.len(){
            for sr in sig_list.iter_mut(){
                
                if sr.sig.is_none() {
                    let f_client = self.rpc.lock();
                    let client = f_client.await;
                    let f_sig_result = client.send_transaction_with_config(&sr.tx,RpcSendTransactionConfig{
                        encoding:Option::default(),
                        skip_preflight:true,
                        preflight_commitment:Some(CommitmentLevel::Finalized),
                        max_retries:Some(1), 
                        min_context_slot: Some(10),
                    } );
                    let sig_result=f_sig_result.await;
                    if sig_result.is_err(){
                        return Err(Status::internal("no signature"));
                    }
                    sr.set_sig(sig_result.unwrap().clone());
                }
                
                if !sr.in_system {
                    let mut x:Vec<u8>=Vec::new();
                    for k in sr.serialized_tx.iter(){
                        x.push(*k);
                    }
                    let f_tpu_client = self.tpu.lock();
                    let tpu_client = f_tpu_client.await;
                    if !tpu_client.send_wire_transaction(x){
                        return Err(Status::internal("failed to transmit"));
                    }
                    sleep(Duration::from_millis(10));
                    let f_client = self.rpc.lock();
                    let client = f_client.await;
                    
                    if let Some(x)=&sr.sig.clone(){
                        let f_r = client.confirm_transaction_with_commitment(
                            x,
                             CommitmentConfig{ commitment: CommitmentLevel::Finalized }
                        );
                        let r = f_r.await;
                        if r.is_err(){
                            return Err(Status::internal("confirmation check failed"));
                        }
                        let response = r.unwrap();
                        if response.value {
                            sr.in_system=true;
                            confirmed_tx_count=confirmed_tx_count+1;
                        }
                    } else {
                        return Err(Status::internal("failed to get signature"));
                    }
                    
                }
            }
        }

        let ans= SendBatchResponse {
            signature:gen_sig_list(sig_list),
        };


        return Ok(Response::new(ans));
    }
    async fn run_genesis<'a>(&'a self, req: Request<Genesis>) -> Result<Response<solmate_rust_helper::basic::Empty>,Status> { 
        let r = req.get_ref();
        let payer;
        if let Some(x)=copy_keypair(&r.payer){
            match convert_keypair(&x){
                Ok(y)=>{
                    payer=y;
                },
                Err(e)=>{
                    return Err(Status::invalid_argument("no payer"));
                }
            }
        } else {
            return Err(Status::invalid_argument("no payer"));
        }

        let mint;
        if let Some(x)=copy_keypair(&r.mint){
            match convert_keypair(&x){
                Ok(y)=>{
                    mint=y;
                },
                Err(e)=>{
                    return Err(Status::invalid_argument("no mint"));
                }
            }
        } else {
            return Err(Status::invalid_argument("no mint"));
        }

        let owner;
        let o_1 = r.owner.clone();
        if let Some(x)=o_1{
            match convert_pubkey(&x){
                Ok(y)=>{
                    owner=y;
                },
                Err(e)=>{
                    return Err(Status::invalid_argument("no owner"));
                }
            }
        } else {
            return Err(Status::invalid_argument("no owner"));
        }

        if r.decimals.len() != 1 {
            return Err(Status::invalid_argument("no decimals"));
        }
        let decimals = r.decimals[0];

        
        let x = rpcnb::create_and_init_mint(self.rpc.clone(),&payer,&mint,&owner,decimals).await;

        match x{
            Ok(y)=>{
                return Ok(Response::new(basic::Empty{}))
            },
            Err(e)=>{
                return Err(Status::internal("failed to create and init mint"))
            }
        }

    }

    async fn run_initialize_token_account<'a>(&'a self, req: Request<InitializeTokenAccount>) -> Result<Response<basic::Keypair>,Status> { 
        let r = req.get_ref();
        
        let mint;
        {
            let m = r.mint.clone();
            if let Some(x)=m{
                match convert_pubkey(&x){
                    Ok(y)=>{
                        mint=y;
                    },
                    Err(e)=>{
                        return Err(Status::invalid_argument("no mint"));
                    }
                }
            } else {
                return Err(Status::invalid_argument("no mint"));
            }
        }
        
        
        let owner;
        {
            if let Some(x)=copy_keypair(&r.owner_account){
                match convert_keypair(&x){
                    Ok(y)=>{
                        owner=y;
                    },
                    Err(e)=>{
                        return Err(Status::invalid_argument("no owner"));
                    }
                }
            } else {
                return Err(Status::invalid_argument("no owner"));
            }
        }
        
        
        let r_1 = self.rpc.clone();
        let mint_1=mint.clone();
        let owner_1 = Keypair::from_bytes(owner.to_bytes().as_ref()).unwrap();
        let result = util::initialize_token_account(r_1, &mint_1, &owner_1).await;
        if result.is_err(){
            return Err(Status::invalid_argument("bad key pair"));
        }
        
        let k=solmate_rust_helper::reverse_keypair(&result.unwrap());
        if k.is_err(){
            return Err(Status::invalid_argument("bad key pair"));
        }
        let y = k.unwrap();
        let z = Response::new(y);
        return Ok(z);
        
    }
    async fn run_create_account<'a>(&'a self, req: Request<CreateAccount>) -> Result<Response<basic::Keypair>,Status> { 
        
        let r = req.get_ref();

        let mint;
        {
            let m = r.mint.clone();
            if let Some(x)=m{
                match convert_pubkey(&x){
                    Ok(y)=>{
                        mint=y;
                    },
                    Err(e)=>{
                        return Err(Status::invalid_argument("no mint"));
                    }
                }
            } else {
                return Err(Status::invalid_argument("no mint"));
            }
        }

        let owner;
        {
            let m = r.owner.clone();
            if let Some(x)=m{
                match convert_pubkey(&x){
                    Ok(y)=>{
                        owner=y;
                    },
                    Err(e)=>{
                        return Err(Status::invalid_argument("no mint"));
                    }
                }
            } else {
                return Err(Status::invalid_argument("no mint"));
            }
        }
        
        let payer;
        {
            if let Some(x)=copy_keypair(&r.payer){
                match convert_keypair(&x){
                    Ok(y)=>{
                        payer=y;
                    },
                    Err(e)=>{
                        return Err(Status::invalid_argument("no payer"));
                    }
                }
            } else {
                return Err(Status::invalid_argument("no payer"));
            }
        }

        let result = util::create_account(self.rpc.clone(), &mint, &owner, &payer).await;
        if result.is_err(){
            return Err(Status::internal("failed to create account"));
        }
        

        let y = solmate_rust_helper::reverse_keypair(&result.unwrap())?;

        return Ok(Response::new(y));

    }
    async fn run_mint<'a>(&'a self, req: Request<Mint>) -> Result<Response<solmate_rust_helper::basic::Empty>,Status> { 
        let r = req.get_ref();

        let payer;
        {
            if let Some(x)=copy_keypair(&r.payer){
                match convert_keypair(&x){
                    Ok(y)=>{
                        payer=y;
                    },
                    Err(e)=>{
                        return Err(Status::invalid_argument("no payer"));
                    }
                }
            } else {
                return Err(Status::invalid_argument("no payer"));
            }
        }
        let signer;
        {        
            if let Some(x)=copy_keypair(&r.signer){
                match convert_keypair(&x){
                    Ok(y)=>{
                        signer=y;
                    },
                    Err(e)=>{
                        return Err(Status::invalid_argument("no signer"));
                    }
                }
            } else {
                return Err(Status::invalid_argument("no signer"));
            }
        
        }
        let mint;
        {
            let m = r.mint.clone();
            if let Some(x)=m{
                match convert_pubkey(&x){
                    Ok(y)=>{
                        mint=y;
                    },
                    Err(e)=>{
                        return Err(Status::invalid_argument("no mint"));
                    }
                }
            } else {
                return Err(Status::invalid_argument("no mint"));
            }
        }
        let recipient;
        {
            let m = r.recipient.clone();
            if let Some(x)=m{
                match convert_pubkey(&x){
                    Ok(y)=>{
                        recipient=y;
                    },
                    Err(e)=>{
                        return Err(Status::invalid_argument("no recipient"));
                    }
                }
            } else {
                return Err(Status::invalid_argument("no recipient"));
            }
        }
        
        let quantity=r.quantity;
        if quantity==0{
            return Err(Status::invalid_argument("no quantity"));
        }

        if util::mint_to_existing_account(self.rpc.clone(), &payer, &signer, &mint, &recipient, quantity).await.is_err(){
            return Err(Status::internal("failed to mint"));
        }
        
        return Ok(Response::new(basic::Empty{}));
    }
}

impl MyServer {

    fn new(
        my_config: solmate_rust_helper::config::Configuration
    ) -> MyServer {

        let rpc_url = my_config.validator_url_http;
        let ws_url = my_config.validator_url_ws;
        let rpc_client: RpcClient = RpcClient::new(rpc_url.clone());
        let mut tpu_config=TpuClientConfig::default();
        tpu_config.fanout_slots=20;

        let rpc_client_blocking: solana_client::rpc_client::RpcClient = solana_client::rpc_client::RpcClient::new(rpc_url.clone());


        let tpu_client = TpuClient::new(Arc::new(rpc_client_blocking), &ws_url, tpu_config).unwrap();

        let r_1 = Arc::new(Mutex::new(rpc_client));
        let t_1 = Arc::new(Mutex::new(tpu_client));
        
        
        return MyServer {
            cluster: my_config.cluster,
            rpc: r_1,
            tpu: t_1,
        }
    }
}



pub fn init(my_config: solmate_rust_helper::config::Configuration)->BroadcastServer<MyServer>{    
    
    return BroadcastServer::new(MyServer::new(my_config));
}


pub struct SendResult{
    in_system: bool,
    sig:  Option<Signature>,
    tx: Transaction,
    serialized_tx: Vec<u8>,
}

impl SendResult {
    fn new(tx: Transaction, serialized_tx: Vec<u8>) -> SendResult{
        return SendResult{
            in_system: false,
            sig: None,
            tx: tx,
            serialized_tx: serialized_tx,
        }
    }

    fn set_sig(&mut self,sig: Signature){
        self.sig=Some(sig);
    }
}

fn gen_sig_list(x: Vec<SendResult>)->Vec<Vec<u8>>{
    let mut list: Vec<Vec<u8>>=Vec::new();
    for sr in x.iter(){
        if sr.sig.is_none(){
            let mut z: Vec<Vec<u8>> = vec![vec![]];
            list.append(&mut z);
        } else {
            let y: [u8; 64]=sr.sig.unwrap().into();
            let mut z: Vec<Vec<u8>> = vec![vec![]];
            z[0]=y.to_vec().clone();
            list.append(&mut z);
        }
    }
    return list
}


