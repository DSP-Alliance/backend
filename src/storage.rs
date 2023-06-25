use jsonrpc::Response;
use redis::{FromRedisValue, ToRedisArgs};
use reqwest::Client;
use serde_json::{json, Value};
use thiserror::Error;

const MAINNET_RPC: &str = "https://api.chain.love/rpc/v0";
const TESTNET_RPC: &str = "https://filecoin-calibration.chainup.net/rpc/v1";

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Network {
    Mainnet,
    Testnet,
}

#[derive(Debug, Error)]
pub enum StorageFetchError {
    #[error("reqwest error")]
    Reqwest(#[from] reqwest::Error),
    #[error("serde error")]
    Serde(#[from] serde_json::Error),
    #[error("no result")]
    NoResult,
}

pub async fn verify_id(
    id: String,
    worker_address: String,
    ntw: Network,
) -> Result<bool, StorageFetchError> {
    let client = Client::new();

    let rpc = ntw.rpc();

    let response = client
        .post(rpc)
        .header("Content-Type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "Filecoin.StateMinerInfo",
            "params": [
                id,
                null
            ],
            "id": 1
        }))
        .send()
        .await?
        .json::<Response>()
        .await?;

    let worker_id = match response.result {
        Some(w) => {
            let parsed_result: Value = serde_json::from_str(w.to_string().as_str())?;

            if let Some(worker_id) = parsed_result["Worker"].as_str() {
                worker_id.to_string()
            } else {
                return Ok(false);
            }
        }
        None => return Ok(false),
    };

    let response = client
        .post(rpc)
        .header("Content-Type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "Filecoin.StateAccountKey",
            "params": [
                worker_id,
                null
            ],
            "id": 1
        }))
        .send()
        .await?
        .json::<Response>()
        .await?;

    match response.result {
        Some(w) => {
            let parsed_result: Value = serde_json::from_str(w.to_string().as_str())?;

            if let Some(rec_worker_address) = parsed_result.as_str() {
                Ok(rec_worker_address == worker_address)
            } else {
                Ok(false)
            }
        }
        None => Ok(false),
    }
}

pub async fn fetch_storage_amount(sp_id: u32, ntw: Network) -> Result<u128, StorageFetchError> {
    let client = Client::new();
    let rpc = match ntw {
        Network::Mainnet => MAINNET_RPC,
        Network::Testnet => TESTNET_RPC,
    };
    let sp_id = sp_id_format(ntw, sp_id);
    let response = client
        .post(rpc)
        .header("Content-Type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "Filecoin.StateMinerPower",
            "params": [
                sp_id,
                null
            ],
            "id": 1
        }))
        .send()
        .await?
        .json::<Response>()
        .await?;

    match response.result {
        Some(result) => {
            let parsed_result: Value = serde_json::from_str(result.to_string().as_str())?;

            if let Some(power) = parsed_result["MinerPower"]["RawBytePower"].as_str() {
                Ok(power.parse::<u128>().unwrap())
            } else {
                Err(StorageFetchError::NoResult)
            }
        }
        None => Err(StorageFetchError::NoResult),
    }
}

fn sp_id_format(ntw: Network, id: u32) -> String {
    match ntw {
        Network::Mainnet => format!("f0{}", id),
        Network::Testnet => format!("t0{}", id),
    }
}

impl Network {
    pub fn rpc(&self) -> &'static str {
        match self {
            Network::Mainnet => MAINNET_RPC,
            Network::Testnet => TESTNET_RPC,
        }
    }
}

impl ToRedisArgs for Network {
    fn write_redis_args<W: ?Sized>(&self, out: &mut W)
    where
        W: redis::RedisWrite,
    {
        match self {
            Network::Mainnet => "mainnet".write_redis_args(out),
            Network::Testnet => "testnet".write_redis_args(out),
        }
    }
}

impl FromRedisValue for Network {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        let s = String::from_redis_value(v)?;
        match s.as_str() {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            _ => Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Unknown network",
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn storage_fetch_storage_amount_mainnet() {
        let res = fetch_storage_amount(1240u32, Network::Mainnet).await;

        println!("{:?}", res);
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn storage_fetch_storage_amount_testnet() {
        let res = fetch_storage_amount(6024u32, Network::Testnet).await;

        println!("{:?}", res);
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn storage_verify_id_testnet() {
        let res = verify_id("t06024".to_string(), "t3qejyqmrirddrsb2w2thbaco3q6emuljumlhuonp3al35g3kkzx4zpeecycw7gim2meegemwot3gp3qr6alpa".to_string(), Network::Testnet).await.unwrap();

        assert!(res);
    }

    #[tokio::test]
    async fn storage_verify_id_mainnet() {
        let res = verify_id("f01240".to_string(), "f3wzxynjiptyogm442qg4cv74czijfzj7fzymqx6gmr6yw6oojhmlg7qavplholgoeyiyxh2zostfrnc2w2mxq".to_string(), Network::Mainnet).await.unwrap();

        assert!(res);
    }
}
