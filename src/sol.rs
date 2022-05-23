
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
    }
};
use solana_client::{
    nonblocking::rpc_client::RpcClient, 
    tpu_client::{
        TpuClientConfig, TpuClient,
    }
};
use solmate_client::{
    rpcnb,
    convert_pubkey,convert_keypair,reverse_keypair, copy_keypair
};
use tonic::{Request, Response, Status};
use solmate_client::{config,solanabase,util,basic};

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
    async fn send_tx<'a>(&'a self, req: Request<SendBatchRequest>) -> Result<Response<SendBatchResponse>,Status> { 
        todo!() 
    }
    async fn run_genesis<'a>(&'a self, req: Request<Genesis>) -> Result<Response<solmate_client::basic::Empty>,Status> { 
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
        
        let k=solmate_client::reverse_keypair(&result.unwrap());
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
        

        let y = solmate_client::reverse_keypair(&result.unwrap())?;

        return Ok(Response::new(y));

    }
    async fn run_mint<'a>(&'a self, req: Request<Mint>) -> Result<Response<solmate_client::basic::Empty>,Status> { 
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
        my_config: solmate_client::config::Configuration
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


pub fn init<'a>(my_config: solmate_client::config::Configuration)->BroadcastServer<MyServer>{
    return BroadcastServer::new(MyServer::new(my_config));
}




