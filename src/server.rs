use std::sync::Arc;

use futures::lock::Mutex;
use solana_client::{nonblocking::rpc_client::RpcClient, tpu_client::{TpuClientConfig, TpuClient}};
use tonic::transport::Server;

mod serum;
mod sol;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let my_config = solmate_rust_helper::config::config_from_env();

    let addr = my_config.grpc_listen_url.parse()?;
    let serum_server = serum::init(my_config.clone()).await;




    Server::builder().concurrency_limit_per_connection(64)
        .add_service(serum_server)
        .add_service(sol::init(my_config.clone()))
        .serve(addr)
        .await.unwrap();

    Ok(())
}