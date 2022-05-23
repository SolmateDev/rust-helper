use solana_sdk::{
    signature::Signature,
    clock::MAX_PROCESSING_AGE,
};
use solana_client::rpc_config::RpcSendTransactionConfig;

use tonic::{Request, Response, Status};
use bincode::{
    deserialize,
    Result as BincodeResult
};
use std::env;

use solmate_client::config;
use solmate_client::solananet;
use solananet::{
    broadcast_server::{
        BroadcastServer, Broadcast,
    },
    SendBatchRequest,SendBatchResponse,
};

use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
};



use std::{
    time::{Duration, Instant},
    thread::sleep,
    //net::UdpSocket,
    sync::Arc
};


use {
    
    solana_sdk::{
        transaction::Transaction,
        
    },
    solana_client::{
        tpu_client::TpuClient,
        //quic_client::QuicTpuConnection,
        rpc_client::{
            RpcClient,
        },
        tpu_client::TpuClientConfig,
    }
};


//#[derive(Default)]
pub struct MyServer {
    pub rpc: RpcClient,
    pub tpu: TpuClient,
}


fn create_server(
    rpc_client: RpcClient,
    tpu_client: TpuClient,
) -> MyServer {

    MyServer {
        rpc: rpc_client,
        tpu: tpu_client,
    }
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

#[tonic::async_trait]
impl Broadcast for MyServer {

    async fn send_tx(&self, msg: Request<SendBatchRequest>)->Result<Response<SendBatchResponse>,Status>{
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
                    let sig_result = self.rpc.send_transaction_with_config(&sr.tx,RpcSendTransactionConfig{
                        encoding:Option::default(),
                        skip_preflight:true,
                        preflight_commitment:Some(CommitmentLevel::Finalized),
                        max_retries:Some(1), 
                        min_context_slot: Some(10),
                    } );
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
                    if !self.tpu.send_wire_transaction(x){
                        return Err(Status::internal("failed to transmit"));
                    }
                    sleep(Duration::from_millis(10));
                    let r = self.rpc.confirm_transaction_with_commitment(&sr.sig.unwrap(), CommitmentConfig::processed());
                    if r.is_err(){
                        return Err(Status::internal("confirmation check failed"));
                    }
                    let response = r.unwrap();
                    if response.value {
                        sr.in_system=true;
                        confirmed_tx_count=confirmed_tx_count+1;
                    }
                }
            }
        }

        let ans= SendBatchResponse {
            signature:gen_sig_list(sig_list),
        };


        return Ok(Response::new(ans));
    }


}


pub fn init(my_config: solmate_client::config::Configuration)->BroadcastServer<MyServer>{    
    let rpc_url = my_config.validator_url_http;
    let ws_url = my_config.validator_url_ws;
    let rpc_client = RpcClient::new(rpc_url.clone());
    let mut tpu_config=TpuClientConfig::default();
    tpu_config.fanout_slots=20;

    let tpu_client = TpuClient::new(Arc::new(rpc_client), &ws_url, tpu_config).unwrap();

    return BroadcastServer::new(create_server(RpcClient::new(rpc_url),tpu_client));
}