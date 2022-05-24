use std::env;
use tonic::{Status};

#[derive(Copy, Clone)]
pub enum Cluster{
    Mainet, Testnet,Localnet
}


pub struct Configuration{
    pub cluster: Cluster,
    pub grpc_listen_url: String, // example:  "0.0.0.0:50051"
    pub validator_url_http: String, // http://127.0.0.1:8899
    pub validator_url_ws: String, // ws://127.0.0.1:8900
}

impl Configuration{
    pub fn clone(&self) -> Configuration{
        return Configuration{
            cluster: self.cluster,
            grpc_listen_url: self.grpc_listen_url.clone(),
            validator_url_http: self.validator_url_http.clone(),
            validator_url_ws: self.validator_url_ws.clone(),
        }
    }
}

fn config_default_cluster(err: std::env::VarError)->Result<String,String>{
    return Ok(String::from("mainnet"));
}


pub fn parse_cluster(name: String)->Result<Cluster,Status>{
    match name.as_ref(){
        "mainnet"=>{
            return Ok(Cluster::Mainet);
        },
        "testnet"=>{
            return Ok(Cluster::Testnet);
        },
        "localnet"=>{
            return Ok(Cluster::Localnet);
        },
        _=>{
            return Err(Status::internal("no such cluster"));
        }
    }
}

pub fn config_from_env() ->Configuration{

    let cluster_str=env::var("CLUSTER").or_else(config_default_cluster).unwrap();
    let cluster=parse_cluster(cluster_str).unwrap();
    let grpc_name=env::var("GRPC_LISTEN_URL").expect("$GRPC_LISTEN_URL is not set");
    let v_http=env::var("VALIDATOR_HTTP_URL").expect("$VALIDATOR_HTTP_URL is not set");
    let v_ws=env::var("VALIDATOR_WS_URL").expect("$VALIDATOR_WS_URL is not set");
    return Configuration{
        cluster,
        grpc_listen_url:grpc_name.to_string(),
        validator_url_http:v_http.to_string(),
        validator_url_ws:v_ws.to_string(),
    }
}