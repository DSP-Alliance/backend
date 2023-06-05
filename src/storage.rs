use bls_signatures::PublicKey;
use jsonrpc::Response;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

const CHAIN_LOVE: &str = "https://api.chain.love/rpc/v0";

#[derive(Deserialize, Debug)]
struct Results {
    #[serde(rename = "MinerPower")]
    miner_power: MinerPower,
}

#[derive(Deserialize, Debug)]
pub struct MinerPower {
    #[serde(rename = "RawBytePower")]
    pub raw_byte_power: String,
}

pub async fn verify_id(id: String) -> Result<bool, reqwest::Error> {
    let client = Client::new();
    let response = client
        .post(CHAIN_LOVE)
        .header("Content-Type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "Filecoin.StateLookupID",
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

    println!("{:?}", response);

    Ok(false)
}

pub async fn fetch_storage_amount(sp_id: String) -> Result<MinerPower, StorageFetchError> {
    let client = Client::new();
    let response = client
        .post(CHAIN_LOVE)
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

    let result = match response.result {
        Some(result) => result,
        None => return Err(StorageFetchError::NoResult),
    };

    let res = serde_json::from_str::<Results>(result.to_string().as_str()).unwrap();

    Ok(res.miner_power)
}

#[derive(Debug)]
pub enum StorageFetchError {
    Reqwest(reqwest::Error),
    Serde(serde_json::Error),
    NoResult
}

impl From<reqwest::Error> for StorageFetchError {
    fn from(e: reqwest::Error) -> Self {
        StorageFetchError::Reqwest(e)
    }
}

impl From<serde_json::Error> for StorageFetchError {
    fn from(e: serde_json::Error) -> Self {
        StorageFetchError::Serde(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_storage_amount() {
        let res = fetch_storage_amount("f01240".to_string()).await.unwrap();

        println!("{:?}", res);
    }

    #[tokio::test]
    async fn test_verify_id() {
        let res = verify_id("t06016".to_string()).await.unwrap();

        println!("{:?}", res);
    }
}