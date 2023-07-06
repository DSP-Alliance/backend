pub mod redis;
pub mod storage;
pub mod messages {
    pub mod auth;
    pub mod vote_registration;
    pub mod vote_start;
    pub mod votes;
}
pub mod errors;
pub mod get;
pub mod post;

use std::str::FromStr;

use clap::{arg, command, Parser};
use ethers::types::Address;
use serde::Deserialize;
use url::Url;

const STARTING_AUTHORIZED_VOTERS: [&str; 3] = [
    "0x3B9705F0EF88Ee74B9924e34A5Af578d2E24F300",
    "0xf2361d2a9a0677e8ffd1515d65cf5190ea20eb56",
    "0x47f033Ed0F9485677008dC30507273607A74E92C",
    "0xe662D77E7e3096683BAC8f1Ad526FB033E3810eB"
];

// Default values for command line arguments
const VOTE_LENGTH: &str = "60";
const REDIS_DEFAULT_PATH: &str = "redis://127.0.0.1:6379";
const DEFAULT_SERVE_ADDRESS: &str = "http://127.0.0.1:51634";

#[derive(Parser, Clone)]
#[command(name = "filecoin-vote")]
pub struct Args {
    #[arg(short, long, default_value = DEFAULT_SERVE_ADDRESS)]
    pub serve_address: Url,
    #[arg(short, long, default_value = REDIS_DEFAULT_PATH)]
    pub redis_path: Url,
    #[arg(short, long, default_value = VOTE_LENGTH)]
    pub vote_length: u64,
}

impl Default for Args {
    fn default() -> Self {
        Self::new()
    }
}

impl Args {
    pub fn new() -> Self {
        Self::parse()
    }

    pub fn vote_length(&self) -> u64 {
        self.vote_length
    }

    pub fn redis_path(&self) -> Url {
        self.redis_path.clone()
    }

    pub fn serve_address(&self) -> Url {
        self.serve_address.clone()
    }
}

#[derive(Deserialize)]
pub struct NtwFipParams {
    network: String,
    fip_number: u32,
}

#[derive(Deserialize)]
pub struct NtwAddrParams {
    network: String,
    address: String,
}

#[derive(Deserialize)]
pub struct FipParams {
    fip_number: u32,
}

#[derive(Deserialize)]
pub struct NtwParams {
    network: String,
}

pub fn authorized_voters() -> Vec<Address> {
    STARTING_AUTHORIZED_VOTERS
        .iter()
        .map(|s| Address::from_str(s).unwrap())
        .collect()
}
