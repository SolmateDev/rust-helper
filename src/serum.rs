
use serum_dex::instruction::NewOrderInstructionV3;
use solana_client::rpc_response::RpcResponseContext;
use solana_sdk::account::Account;

use solmate_client::basic::{SignedTx};
use solmate_client::serum::{  ListMarketResponse, ListMarketRequest, ConsumeEventsRequest, InitOpenOrderRequest, MarketState, CancelOrderRequest, SettleFundsRequest, CloseOpenOrderRequest, MarketRequest, MatchOrdersRequest, MonitorQueueRequest};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc::{self, Sender};
use tonic::{Request, Response, Status};
use std::num::NonZeroU64;
use futures::lock::Mutex;
use std::{env, thread};
use std::pin::Pin;
use tokio_stream::wrappers::ReceiverStream;
//use solana_rpc::rpc_pubsub::gen_client::Client as PubsubClient;
use solmate_client::{
    convert_keypair,convert_pubkey, copy_keypair, reverse_pubkey,
};
use solmate_client::util::{
    pubkey_from_bytes, keypair_from_bytes,
};
use solmate_client::config;

use solmate_client::serum::{
    dex_server::{
        DexServer, Dex,
    },
    ConsumeEventUpdate,
    
     WholeShebang, 
    MarketPubkeys as PbMarketPubkeys,
    Event,
    OrderStatus, Order,
};

use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    pubkey::{
        Pubkey,
    },
    signer::keypair::{
        Keypair,
    },
    transaction::{Transaction},
};
use serum_common::client::rpc::{
    create_and_init_mint, create_token_account, mint_to_new_account, send_txn, simulate_transaction,
};

use std::{
    time::{Duration, Instant},
    thread::sleep,
    sync::Arc
};
use anyhow::{anyhow,Result};
use solana_client::{
    tpu_client::TpuClient,
    nonblocking::{
        rpc_client::RpcClient,
        pubsub_client::{
            PubsubClient,
        }
    },
    tpu_client::TpuClientConfig,
    rpc_config::RpcSignatureSubscribeConfig,
};

use bincode::serialize;
use jsonrpc_core::futures::StreamExt;

use self::dex::MarketPubkeys;
//use solana_account_decoder::UiAccount;

mod dex;



#[derive()]
pub struct MyServer {
    cluster: config::Cluster,
    pub rpc: Arc<Mutex<RpcClient>>,
    pub tpu: Arc<Mutex<TpuClient>>,
    pub ws: Arc<Mutex<PubsubClient>>,
    pub rt: Arc<Mutex<Runtime>>,
}

pub fn convert_market_pubkeys(ans: &dex::MarketPubkeys)->PbMarketPubkeys{

    return PbMarketPubkeys{
        market: Some(reverse_pubkey(ans.market.as_ref())),
        req_q: Some(reverse_pubkey(ans.req_q.as_ref())),
        event_q: Some(reverse_pubkey(ans.event_q.as_ref())),
        bids: Some(reverse_pubkey(ans.bids.as_ref())),
        asks: Some(reverse_pubkey(ans.asks.as_ref())),
        coin_vault: Some(reverse_pubkey(ans.coin_vault.as_ref())),
        pc_vault: Some(reverse_pubkey(ans.pc_vault.as_ref())),
        vault_signer_key: Some(reverse_pubkey(ans.vault_signer_key.as_ref())),
    };
}

pub fn reverse_market_pubkeys<'a>(ans: &'a PbMarketPubkeys)->Result<dex::MarketPubkeys>{
    let market=match &ans.market{
        Some(x)=>convert_pubkey(x)?,
        None=>{
            return Err(anyhow!("bad variable"));
        }
    };
    let req_q = match &ans.req_q{
        Some(x)=>convert_pubkey(x)?,
        None=>{
            return Err(anyhow!("bad variable"));
        }
    };
    let event_q = match &ans.event_q{
        Some(x)=>convert_pubkey(x)?,
        None=>{
            return Err(anyhow!("bad variable"));
        }
    };
    let bids = match &ans.bids{
        Some(x)=>convert_pubkey(x)?,
        None=>{
            return Err(anyhow!("bad variable"));
        }
    };
    let asks = match &ans.asks{
        Some(x)=>convert_pubkey(x)?,
        None=>{
            return Err(anyhow!("bad variable"));
        }
    };
    let coin_vault = match &ans.coin_vault{
        Some(x)=>convert_pubkey(x)?,
        None=>{
            return Err(anyhow!("bad variable"));
        }
    };
    let pc_vault = match &ans.pc_vault{
        Some(x)=>convert_pubkey(x)?,
        None=>{
            return Err(anyhow!("bad variable"));
        }
    };
    let vault_signer_key = match &ans.vault_signer_key{
        Some(x)=>convert_pubkey(x)?,
        None=>{
            return Err(anyhow!("bad variable"));
        }
    };
    
    return Ok(dex::MarketPubkeys{
        market:Box::new(market),
        req_q:Box::new(req_q),
        event_q:Box::new(event_q),
        bids:Box::new(bids),
        asks:Box::new(asks),
        coin_vault:Box::new(coin_vault),
        pc_vault: Box::new(pc_vault),
        vault_signer_key: Box::new(vault_signer_key),
    })
}

pub fn convert_level<'a>(
    level: CommitmentLevel
)->Result<solmate_client::basic::TxStatus,Status>{
    if CommitmentLevel::Processed==level {
        return Ok(solmate_client::basic::TxStatus::Processed);
    } else if CommitmentLevel::Confirmed==level{
        return Ok(solmate_client::basic::TxStatus::Confirmed);
    } else if CommitmentLevel::Finalized==level{
        return Ok(solmate_client::basic::TxStatus::Finalized);
    } else {
        return Err(Status::internal("bad conversion for confirmation level"))
    }
}

pub fn convert_new_order_v3<'a >(
    req: &'a Order,
)->Result<NewOrderInstructionV3,Status>{
    let order;
    match &req.order{
        Some(solmate_client::serum::order::Order::V3(y))=>{
            order=y;
        },
        None=>{
            return Err(Status::invalid_argument("no order"));
        }
    };
    let side;
    if order.side {
        side = serum_dex::matching::Side::Bid;
    } else {
        side = serum_dex::matching::Side::Ask;
    }

    
    let limit_price = NonZeroU64::new(order.limit_price).ok_or(Status::invalid_argument("limit_price is zero"))?;

    let max_coin_qty = NonZeroU64::new(order.max_coin_qty).ok_or(Status::invalid_argument("max_coin_qty is zero"))?;

    let max_native_pc_qty_including_fees = NonZeroU64::new(order.max_native_pc_qty_including_fees).ok_or(Status::invalid_argument("max_coin_qty is zero"))?;

    let self_trade_behavior;
    if order.self_trade_behavior == solmate_client::serum::SelfTradeBehavior::AbortTransaction as i32{
        self_trade_behavior=serum_dex::instruction::SelfTradeBehavior::AbortTransaction
    } else if order.self_trade_behavior == solmate_client::serum::SelfTradeBehavior::CancelProvide as i32 {
        self_trade_behavior=serum_dex::instruction::SelfTradeBehavior::CancelProvide
    } else if order.self_trade_behavior == solmate_client::serum::SelfTradeBehavior::DecrementTake as i32 {
        self_trade_behavior=serum_dex::instruction::SelfTradeBehavior::DecrementTake
    } else {
        return Err(Status::invalid_argument("bad self trade behavior"));
    }

    let order_type;
    if order.order_type == solmate_client::serum::OrderType::ImmediateOrCancel as i32{
        order_type = serum_dex::matching::OrderType::ImmediateOrCancel;
    } else if order.order_type == solmate_client::serum::OrderType::Limit as i32 {
        order_type = serum_dex::matching::OrderType::Limit;
    } else if order.order_type == solmate_client::serum::OrderType::PostOnly as i32{
        order_type = serum_dex::matching::OrderType::PostOnly;
    } else {
        return Err(Status::invalid_argument("bad order type"));
    }
    
    if std::u16::MAX as u32 <= order.limit {
        return Err(Status::invalid_argument("bad limit type"));
    }
    let limit = order.limit as u16;

    // return Err(Status::invalid_argument("no v3 order"));
    return Ok(NewOrderInstructionV3{
        side,
        limit_price,
        max_coin_qty,
        max_native_pc_qty_including_fees,
        self_trade_behavior,
        order_type,
        client_order_id: order.client_order_id,
        limit,
    });
}

async fn convert_state<'a>(client: Arc<Mutex<RpcClient>>,program_id: &'a Pubkey,req: &'a MarketState)->Result<MarketPubkeys>{
    let state;
    let errmsg="bad market state";
    match &req.market{
        Some(solmate_client::serum::market_state::Market::Id(id))=>{
            let market=convert_pubkey(id).or(Err(anyhow!(errmsg)))?;
            state = dex::get_keys_for_market(client.clone(), &program_id, &market).await.or(Err(anyhow!("failed to get market")));
        },
        Some(solmate_client::serum::market_state::Market::State(pb_state)) => {
            state=reverse_market_pubkeys(pb_state);
        },
        _=>{
            return Err(anyhow!("blank market"));
        }
    }

    return state;
    
}



// how to guide: https://github.com/hyperium/tonic/blob/master/examples/routeguide-tutorial.md

#[tonic::async_trait]
impl Dex for MyServer {

    async fn get_market_pubkeys<'a>(&'a self, req: Request<MarketRequest>) -> Result<Response<PbMarketPubkeys>,Status> {
        let r = req.get_ref();
        let mut errmsg="";

        errmsg="bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;
        errmsg="bad market pubkey";
        let market=convert_pubkey(&r.market.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;
        errmsg="failed to get market";
        let state = dex::get_keys_for_market(self.rpc.clone(), &program_id, &market).await.or(Err(Status::internal(errmsg)))?;
        return Ok(Response::new(convert_market_pubkeys(&state)));
    }

    async fn list_market<'a>(&'a self, req: Request<ListMarketRequest>) -> Result<Response<ListMarketResponse>,Status> { 
        let r = req.get_ref();

        let mut errmsg = "";

        errmsg = "bad payer";
        let payer = convert_keypair(&r.payer.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::invalid_argument(errmsg)))?;
        errmsg = "bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;
        errmsg = "bad coin_mint";
        let coin_mint=convert_pubkey(&r.coin_mint.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad pc_mint";
        let pc_mint=convert_pubkey(&r.pc_mint.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "failed to process";
        let (signed_tx,mp)=dex::list_market(self.rpc.clone(), &program_id, &payer, &coin_mint, &pc_mint, r.coin_lot_size, r.pc_lot_size).await.or(Err(Status::internal(errmsg)))?;
        
        return Ok(Response::new(ListMarketResponse{ market_pubkeys: Some(convert_market_pubkeys(&mp)), tx: Some(SignedTx{ tx: signed_tx }) }));
    }

    async fn consume_events<'a>(&'a self, req: Request<ConsumeEventsRequest>) -> Result<Response<SignedTx>,Status> { 
        let r = req.get_ref();

        let mut errmsg = "";

        errmsg = "bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;
        
        errmsg = "bad payer";
        let payer = convert_keypair(&r.payer.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::invalid_argument(errmsg)))?;

        errmsg = "bad market";
        let state;
        if let Some(pb_state)=&r.market{
            state=convert_state(self.rpc.clone(),&program_id,&pb_state).await.or(Err(Status::invalid_argument(errmsg)))?;
        } else {
            return Err(Status::invalid_argument(errmsg));
        }
        
        errmsg = "bad coin_mint";
        let coin_wallet=convert_pubkey(&r.coin_wallet.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad pc_mint";
        let pc_wallet=convert_pubkey(&r.pc_wallet.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        let signed_tx = dex::consume_events(self.rpc.clone(), &program_id, &payer, &state, &coin_wallet, &pc_wallet).await.or(Err(Status::internal("failed to consume events")))?;

        return Ok(Response::new(SignedTx{ tx: signed_tx }));
    }

    async fn init_open_order<'a>(&'a self, req: Request<InitOpenOrderRequest>)->Result<Response<SignedTx>,Status>{
        let r = req.get_ref();
        
        let mut errmsg = "";

        errmsg = "bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad owner";
        let owner = convert_keypair(&r.owner.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::invalid_argument(errmsg)))?;

        errmsg = "bad market";
        let state;
        if let Some(pb_state)=&r.market{
            state=convert_state(self.rpc.clone(),&program_id,&pb_state).await.or(Err(Status::invalid_argument(errmsg)))?;
        } else {
            return Err(Status::invalid_argument(errmsg));
        }
        errmsg = "bad orders";
        let mut orders;
        if let Some(pb_orders) = &r.orders {
            orders =Some(convert_pubkey(&pb_orders).or(Err(Status::invalid_argument(errmsg)))?);
        } else {
            orders =None;
        }
        let m_orders = &mut orders;
        let signed_tx = dex::init_open_orders(self.rpc.clone(),&program_id,&owner,&state,m_orders).await.or(Err(Status::internal("bad init orders")))?;
        return Ok(Response::new(SignedTx{ tx: signed_tx }));
        //return Err(Status::internal("not implemented yet"));
    }

    async fn new_order<'a>(&'a self, req: Request<Order>)->Result<Response<SignedTx>,Status>{
        let r = req.get_ref();

        let mut errmsg = "";

        errmsg = "bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad payer";
        let payer = convert_keypair(&r.payer.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::invalid_argument(errmsg)))?;

        errmsg = "bad wallet";
        let wallet=convert_pubkey(&r.wallet.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad market";
        let state;
        if let Some(pb_state)=&r.market{
            state=convert_state(self.rpc.clone(),&program_id,&pb_state).await.or(Err(Status::invalid_argument(errmsg)))?;
        } else {
            return Err(Status::invalid_argument(errmsg));
        }

        let mut r_orders;
        if let Some(x)=&r.orders{
            let y = convert_pubkey(x).or(Err(Status::internal(errmsg)))?;
            r_orders = Some(y);
        } else {
            r_orders = None;
        }
        let orders = &mut r_orders;

        let new_order = convert_new_order_v3(r)?;

        let signed_tx = dex::place_order(
            self.rpc.clone(),
            &program_id,
            &payer,
            &wallet,
            &state,
            orders,
            new_order
        ).await.or(Err(Status::internal("failed to place order")))?;
        return Ok(Response::new(SignedTx{ tx: signed_tx }));
    }

    async fn cancel_order<'a>(&'a self, req: Request<CancelOrderRequest>)->Result<Response<SignedTx>,Status>{
        let r = req.get_ref();
        let mut errmsg = "";

        errmsg = "bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad owner";
        let owner = convert_keypair(&r.owner.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::invalid_argument(errmsg)))?;

        errmsg = "bad market";
        let state;
        if let Some(pb_state)=&r.market{
            state=convert_state(self.rpc.clone(),&program_id,&pb_state).await.or(Err(Status::invalid_argument(errmsg)))?;
        } else {
            return Err(Status::invalid_argument(errmsg));
        }

        errmsg="bad orders";
        let orders;
        if let Some(x)=&r.orders{
            let y = convert_pubkey(x).or(Err(Status::internal(errmsg)))?;
            //r_orders = Some(y);
            orders = y;
        } else {
            return Err(Status::invalid_argument(errmsg));
        }
        
        let signed_tx = dex::cancel_order_by_client_order_id(self.rpc.clone(),&program_id,&owner,&state,&orders,r.id).await.or(Err(Status::internal("failed to cancel")))?;

        return Ok(Response::new(SignedTx{tx:signed_tx}));
    }



    async fn settle_funds<'a>(&'a self, req: Request<SettleFundsRequest>) -> Result<Response<SignedTx>,Status> { 
        let r = req.get_ref();

        let mut errmsg = "";

        errmsg = "bad payer";
        let payer = convert_keypair(&r.payer.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::invalid_argument(errmsg)))?;

        errmsg = "bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad market";
        let state;
        if let Some(pb_state)=&r.market{
            state=convert_state(self.rpc.clone(),&program_id,&pb_state).await.or(Err(Status::invalid_argument(errmsg)))?;
        } else {
            return Err(Status::invalid_argument(errmsg));
        }

        errmsg = "bad order";
        let order=convert_pubkey(&r.orders.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad coin_wallet";
        let coin_wallet=convert_pubkey(&r.coin_wallet.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad pc_wallet";
        let pc_wallet=convert_pubkey(&r.pc_wallet.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;


        errmsg = "failed to settle funds";
        let signed_tx;
        match &r.signer{
            Some(x)=>{
                let y = convert_keypair(x).or(Err(Status::invalid_argument(errmsg)))?;
                signed_tx = dex::settle_funds(self.rpc.clone(),&program_id,&payer,&state,Some(&y),&order,&coin_wallet,&pc_wallet).await.or(Err(Status::internal(errmsg)))?;
                
            },
            None=>{
                signed_tx = dex::settle_funds(self.rpc.clone(),&program_id,&payer,&state,None,&order,&coin_wallet,&pc_wallet).await.or(Err(Status::internal(errmsg)))?;
            }
        };
    
        return Ok(Response::new(SignedTx{
            tx:signed_tx,
        }));
    }

    async fn close_open_orders<'a>(&'a self, req: Request<CloseOpenOrderRequest>)->Result<Response<SignedTx>,Status>{
        let r = req.get_ref();
        let mut errmsg = "";

        errmsg = "bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad owner";
        let owner = convert_keypair(&r.owner.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::invalid_argument(errmsg)))?;

        errmsg = "bad market";
        let state;
        if let Some(pb_state)=&r.market{
            state=convert_state(self.rpc.clone(),&program_id,&pb_state).await.or(Err(Status::invalid_argument(errmsg)))?;
        } else {
            return Err(Status::invalid_argument(errmsg));
        }

        let orders;
        if let Some(x)=&r.orders{
            let y = convert_pubkey(x).or(Err(Status::internal(errmsg)))?;
            orders = y;
        } else {
            return Err(Status::invalid_argument("bad order"))
        }
        

        let signed_tx = dex::close_open_orders(self.rpc.clone(),&program_id,&owner,&state,&orders).await.or(Err(Status::internal("failed to close open orders")))?;

        return Ok(Response::new(SignedTx{tx:signed_tx}));
    }

    async fn match_orders<'a>(&'a self, req: Request<MatchOrdersRequest>) -> Result<Response<SignedTx>,Status> { 
        let r = req.get_ref();

        let mut errmsg = "";

        errmsg = "bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad payer";
        let payer = convert_keypair(&r.payer.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::invalid_argument(errmsg)))?;

        errmsg = "bad market";
        let state;
        if let Some(pb_state)=&r.market{
            state=convert_state(self.rpc.clone(),&program_id,&pb_state).await.or(Err(Status::invalid_argument(errmsg)))?;
        } else {
            return Err(Status::invalid_argument(errmsg));
        }

        errmsg = "bad coin_wallet";
        let coin_wallet=convert_pubkey(&r.coin_wallet.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        errmsg = "bad pc_wallet";
        let pc_wallet=convert_pubkey(&r.pc_wallet.clone().ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;

        

        let signed_tx = dex::match_orders(self.rpc.clone(), &program_id, &payer, &state, &coin_wallet, &pc_wallet).await.or(Err(Status::internal("failed to match orders")))?;


        return Ok(Response::new(SignedTx{tx:signed_tx}));

    }

    

    type MonitorQueueStream = ReceiverStream<Result<Event, Status>>;

    async fn monitor_queue<'a>(&'a self, req: Request<MonitorQueueRequest>) -> Result<Response<Self::MonitorQueueStream>,Status> { 
        let r = req.get_ref();

        let y = req.metadata().get("hi");

        let mut errmsg="";

        errmsg = "bad program id";
        let program_id=convert_pubkey(&r.dex_program_id.clone().ok_or(Status::internal(errmsg))?.id.ok_or(Status::internal(errmsg))?).or(Err(Status::internal(errmsg)))?;
        errmsg = "bad market";
        let state;
        if let Some(pb_state)=&r.market{
            state=convert_state(self.rpc.clone(),&program_id,&pb_state).await.or(Err(Status::invalid_argument(errmsg)))?;
        } else {
            return Err(Status::invalid_argument(errmsg));
        }
        

        let (tx, rx) = mpsc::channel(4);

        
        {
            let ws_1=self.ws.clone();
            
            let rt_1=self.rt.clone();
            let tx_1 = tx.clone();
            //let r_1 = self.rpc.clone();
            
            let target = state.bids.clone();

            let handle = rt_1.lock().await.spawn(async move {
                let f_client =ws_1.lock();  
                let client = f_client.await;
                let (mut sub,_sender) = client.account_subscribe(&target, None).await.unwrap();  
                // is client going to be dropped automatically once account_subscribe is finished?
                loop{
                    
                    let msg;

                    if let Some(z)=sub.next().await{
                        if let Some(m)=z.value.decode(){
                            msg = read_event_queue_bid(z.context,m).await;
                        } else {
                            msg = Err(Status::internal("failed to decode"));
                        }
                    } else {
                        msg = Err(Status::internal("failed to sub.next"));
                    }
                    
                    if let Err(e)=tx_1.send(msg).await{
                        eprintln!("{}",e);
                        break;
                    }
                }
            });
            drop(handle);
        }
        

        {
            let ws_1=self.ws.clone();
            
            let rt_1=self.rt.clone();
            let tx_1 = tx.clone();
            //let r_1 = self.rpc.clone();
            
            let target = state.asks.clone();
            
            let handle = rt_1.lock().await.spawn(async move {
                let f_client =ws_1.lock();  
                let client = f_client.await;
                let (mut sub,_sender);
                match client.account_subscribe(&target, None).await {
                    Ok((a,b))=>{
                        (sub,_sender)=(a,b);
                    },
                    Err(e)=>{
                        eprintln!("{}",e);
                        return;
                    }
                }  
                // is client going to be dropped automatically once account_subscribe is finished?
                loop{
                    
                    let msg;

                    if let Some(z)=sub.next().await{
                        if let Some(m)=z.value.decode(){
                            msg = read_event_queue_ask(z.context,m).await;
                        } else {
                            msg = Err(Status::internal("failed to decode"));
                        }
                    } else {
                        msg = Err(Status::internal("failed to sub.next"));
                    }
                    
                    if let Err(e)=tx_1.send(msg).await{
                        eprintln!("{}",e);
                        break;
                    }
                }
            });
            drop(handle);
        }

        return Ok(Response::new(ReceiverStream::new(rx)));
    }
}



async fn read_event_queue_ask(
    context: RpcResponseContext,
    data: Account,
)->Result<Event, Status>{

    //let data:Vec<u8> =value.decode();
    //let event_q_data = value.data;

    return Err(Status::internal("not implemented yet"));
}

async fn read_event_queue_bid(
    context: RpcResponseContext,
    account: Account,
)->Result<Event, Status>{
    //let data:Vec<u8> =value.decode();
    
    return Err(Status::internal("not implemented yet"));
}

impl MyServer {

    fn new(
        cluster: config::Cluster,
        rpc_client: Arc<Mutex<RpcClient>>,
        tpu_client: Arc<Mutex<TpuClient>>,
        ws_client: Arc<Mutex<PubsubClient>>,
    ) -> MyServer {

        let rt = Arc::new(Mutex::new(Builder::new_multi_thread().build().unwrap()));
        return MyServer {
            cluster,
            rpc: rpc_client,
            tpu: tpu_client,
            ws:ws_client,
            rt,
        }
    }


    
    


/* 

    async fn read_queue_length_loop(
        &self,
        r_client: Arc<Mutex<RpcClient>>,
        program_id: Pubkey,
        market: Pubkey,
        port: u16,
    ) -> Result<()> {
        let r_1 = r_client.clone();
        
        let get_data = warp::path("length").map(async {
            
            let market_keys = dex::get_keys_for_market(r_1.clone(), &program_id, &market).await?;
            let event_q_data = client
                .get_account_with_commitment(&market_keys.event_q, CommitmentConfig::recent())
                .unwrap()
                .value
                .expect("Failed to retrieve account")
                .data;
            let inner: Cow<[u64]> = dex::remove_dex_account_padding(&event_q_data).unwrap();
            let (_header, seg0, seg1) = dex::parse_event_queue(&inner).unwrap();
            let len = seg0.len() + seg1.len();
            format!("{{ \"length\": {}  }}", len)
        });

        Ok(warp::serve(get_data).run(([127, 0, 0, 1], port)).await)
    }
    */





    async fn read_queue_length_loop_single<'a >(
        &self,
        program_id: Pubkey,
        market: &'a Pubkey,
        port: u16,
    ) -> Result<()> {
        
        let market_keys = dex::get_keys_for_market(self.rpc.clone(), &program_id, market).await?;

        
        
            
        let client = self.rpc.lock().await;
        
        let x = client
                    .get_account_with_commitment(&market_keys.event_q, CommitmentConfig::processed());
        let event_q_data = x.await?.value.ok_or(anyhow!("no value"))?.data;
        let inner = dex::remove_dex_account_padding(event_q_data.as_ref())?;
        let (event_q_header, seg0, seg1) = dex::parse_event_queue(&inner)?;
        let len = seg0.len() + seg1.len();

        return Ok(());
        
    }
}





pub async fn init(my_config: solmate_client::config::Configuration)->DexServer<MyServer>{    
    let rpc_url = my_config.validator_url_http;
    let ws_url = my_config.validator_url_ws;
    let rpc_client = Arc::new(Mutex::new(RpcClient::new(rpc_url.clone())));
    let mut tpu_config=TpuClientConfig::default();
    tpu_config.fanout_slots=20;

    let rpc_client_blocking: solana_client::rpc_client::RpcClient = solana_client::rpc_client::RpcClient::new(rpc_url.clone());


    let tpu_client = Arc::new(Mutex::new(TpuClient::new(Arc::new(rpc_client_blocking), &ws_url, tpu_config).unwrap()));
    

    //let url = format!("ws://0.0.0.0:{}/", pubsub_addr.port());

    let url =ws_url.clone();
    let ws_client = Arc::new(Mutex::new(PubsubClient::new(&url).await.unwrap()));
    

    return DexServer::new(MyServer::new(my_config.cluster,rpc_client,tpu_client,ws_client));
}