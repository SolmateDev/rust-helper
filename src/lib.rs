use anyhow::{anyhow, Result};
use tonic::{Status as TonicStatus};
use solana_sdk::{
    pubkey::{
        Pubkey,
    },
    signer::keypair::{
        Keypair,
    }
};
pub mod basic {
    tonic::include_proto!("basic");
}
pub mod serum{
    tonic::include_proto!("serum");
}

pub mod solanabase{
    tonic::include_proto!("solana");
}

pub mod config;
pub mod util;
pub mod rpcnb;

use serum::{
    MarketPubkeys as PbMarketPubkeys
};


use util::{
    pubkey_from_bytes, keypair_from_bytes,keypair_to_bytes
};


pub fn reverse_pubkey<'a >(ans: &'a Pubkey)->basic::Pubkey{

    let x = ans.to_bytes();
    let mut data=Vec::new();
    for i in 0..x.len()-1{
        data.push(x[i]);
    }
    return basic::Pubkey{
        data
    }
}

pub fn convert_pubkey<'a>(req: &'a basic::Pubkey)->Result<Pubkey>{
    return pubkey_from_bytes(&req.data);
}

pub fn reverse_keypair<'a>(key: &'a Keypair)->Result<basic::Keypair,TonicStatus>{
    match keypair_to_bytes(key){
        Ok(private_key)=>{
            return Ok(basic::Keypair{
                input: Option::Some(
                    basic::keypair::Input::PrivateKey(private_key)
                ),
            })
        },
        Err(e)=>{
            eprintln!("{}",e);
            return Err(TonicStatus::invalid_argument("bad key pair"));
        }
    }
}

pub fn copy_keypair<'a>(req2: &'a Option<basic::Keypair>)->Option<basic::Keypair>{
    if let Some(req) = req2{
        let i = req.input.clone();
        if let Some(data) = i {
            let x = data.clone();
            return Some(basic::Keypair{
                input: Some(x)
            });
        } else {
            return None;
        }
    } else {
        return None;
    }
    

}

pub fn convert_keypair<'a>(req: &'a basic::Keypair)->Result<Keypair,TonicStatus>{

    let keypair;
    match &req.input{
        Some(basic::keypair::Input::Seed(x))=>{
            return Err(TonicStatus::invalid_argument("seed not implemented"));
        },
        Some(basic::keypair::Input::PrivateKey(data))=>{
            match keypair_from_bytes(data){
                Ok(x)=>{
                    keypair = x;
                },
                Err(e)=>{
                    eprintln!("{}",e);
                    return Err(TonicStatus::invalid_argument("bad key pair"));
                }
            }
        },
        None=>{
            return Err(TonicStatus::invalid_argument("bad key pair"));
        }
    };

    return Ok(keypair);    
}
