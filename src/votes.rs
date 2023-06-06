use std::time;

use redis::{from_redis_value, FromRedisValue, ToRedisArgs};
use regex::Regex;
use serde::Deserialize;
use thiserror::Error;

use bls_signatures::{PublicKey, Signature, Serialize};

extern crate base64;

use crate::storage::{fetch_storage_amount, Network, verify_id};

const YAY: VoteOption = VoteOption::Yay;
const NAY: VoteOption = VoteOption::Nay;
const ABSTAIN: VoteOption = VoteOption::Abstain;

const VOTE_OPTIONS: [u8; 3] = [YAY as u8, NAY as u8, ABSTAIN as u8];

#[derive(Debug, PartialEq)]
pub enum VoteOption {
    Yay,
    Nay,
    Abstain,
}

#[derive(Debug, Error)]
pub enum VoteError {
    #[error("Could not recover vote choice from signature")]
    InvalidVoteOption,
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Invalid public key")]
    InvalidKey,
    #[error("Invalid filecoin address encoding")]
    InvalidAddressEncoding,
    #[error("Invalid signature encoding")]
    InvalidSignatureEncoding,
    #[error("Could not fetch storage size")]
    InvalidStorageFetch,
    #[error("Actor is not a miner")]
    NotMiner,
}

impl From<u8> for VoteOption {
    fn from(byte: u8) -> Self {
        match byte {
            0 => VoteOption::Yay,
            1 => VoteOption::Nay,
            2 => VoteOption::Abstain,
            _ => panic!("Invalid vote option"),
        }
    }
}

#[derive(Debug)]
pub struct Vote {
    pub choice: VoteOption,
    timestamp: u64,
    voter: PublicKey,
    raw_byte_power: u128,
    worker_addr: String,
}

#[derive(Deserialize)]
pub struct RecievedVote {
    signature: String,
    worker_address: String,
    sp_id: String,
}

// 0293eafdcd619bd6ae1a86185fc6dbb2e534fba9086183cb9aa2c3f6feceb9441ecd9297981f1c1d23cffa1730535fc8411298e1650364ca666f4558240ab585af8556b07729b3c3c202fb5ac4477016510f744e768c0d0fce320613e70d64c006
// t3qejyqmrirddrsb2w2thbaco3q6emuljumlhuonp3al35g3kkzx4zpeecycw7gim2meegemwot3gp3qr6alpa
// t01000

impl RecievedVote {
    pub async fn recover_vote(&self) -> Result<Vote, VoteError> {
        let (pubkey, ntw) = self.pub_key()?;

        let sig = self.sig()?;

        let miner = match verify_id(self.sp_id.clone(), self.worker_address.clone(), ntw).await {
            Ok(miner) => miner,
            Err(_) => return Err(VoteError::InvalidSignature),
        };

        if !miner {
            return Err(VoteError::NotMiner);
        }

        let miner_power = match fetch_storage_amount(self.sp_id.clone(), ntw).await {
            Ok(miner_power) => miner_power,
            Err(_) => return Err(VoteError::InvalidSignature),
        };

        for msg in VOTE_OPTIONS {
            match pubkey.verify(sig, &[msg]) {
                true => {
                    return Ok(Vote {
                        choice: msg.into(),
                        timestamp: time::SystemTime::now()
                            .duration_since(time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        voter: pubkey,
                        raw_byte_power: miner_power,
                        worker_addr: self.worker_address.clone(),
                    });
                }
                false => (),
            }
        }

        return Err(VoteError::InvalidVoteOption);
    }

    fn pub_key(&self) -> Result<(PublicKey, Network), VoteError> {
        let testnet_base32 = Regex::new(r"(?i)^[t][3][A-Z2-7]{84}$").unwrap();
        let mainnet_base32 = Regex::new(r"(?i)^[f][3][A-Z2-7]{84}$").unwrap();
        
        let ntw: Network;

        let bytes = match testnet_base32.is_match(&self.worker_address) {
            true => {
                match base32::decode(
                    base32::Alphabet::RFC4648 { padding: false },
                    &self.worker_address[2..self.worker_address.len() - 6],
                ) {
                    Some(bytes) => {
                        ntw = Network::Testnet;
                        bytes
                    },
                    None => return Err(VoteError::InvalidAddressEncoding),
                }
            }
            false => match mainnet_base32.is_match(&self.worker_address) {
                true => {
                    match base32::decode(
                        base32::Alphabet::RFC4648 { padding: false },
                        &self.worker_address[2..self.worker_address.len() - 6],
                    ) {
                        Some(bytes) => {
                            ntw = Network::Mainnet;
                            bytes
                        },
                        None => return Err(VoteError::InvalidAddressEncoding),
                    }
                }
                false => return Err(VoteError::InvalidAddressEncoding),
            },
        };

        match PublicKey::from_bytes(bytes.as_slice()) {
            Ok(pubkey) => Ok((pubkey, ntw)),
            Err(_) => Err(VoteError::InvalidKey),
        }
    }

    fn sig(&self) -> Result<Signature, VoteError> {
        let bytes = match hex::decode(&self.signature[2..]) {
            Ok(bytes) => bytes,
            Err(e) => return {
                println!("error hex decode: {:?}", e);
                Err(VoteError::InvalidSignatureEncoding)
            },
        };

        match Signature::from_bytes(bytes.as_slice()) {
            Ok(sig) => Ok(sig),
            Err(e) => {
                println!("error from bytes: {:?}", e);
                Err(VoteError::InvalidSignature)
            },
        }
    }
}

impl FromRedisValue for VoteOption {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        let s: u8 = from_redis_value(v)?;
        match s {
            0 => Ok(YAY),
            1 => Ok(NAY),
            2 => Ok(ABSTAIN),
            _ => Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Invalid vote option",
            ))),
        }
    }
}

impl ToRedisArgs for VoteOption {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        let val = match self {
            VoteOption::Yay => 0u8,
            VoteOption::Nay => 1u8,
            VoteOption::Abstain => 2u8,
        };

        val.write_redis_args(out);
    }
}

impl FromRedisValue for Vote {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        let args: Vec<u8> = from_redis_value(v)?;

        if args.len() != 159 {
            return Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Invalid vote",
            )));
        }

        let choice: VoteOption = args[0].into();

        let timestamp = u64::from_be_bytes([
            args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8],
        ]);

        let voter = match PublicKey::from_bytes(&args[9..57]) {
            Ok(voter) => voter,
            Err(_) => {
                return Err(redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "Invalid voter key",
                )))
            }
        };

        let raw_byte_power = u128::from_be_bytes([
            args[58], args[59], args[60], args[61], args[62], args[63], args[64], args[65],
            args[66], args[67], args[68], args[69], args[70], args[71], args[72], args[73],
        ]);

        let worker_addr = String::from_utf8(args[73..].to_vec()).unwrap();

        Ok(Vote {
            choice,
            timestamp,
            voter,
            raw_byte_power,
            worker_addr,
        })
    }
}

impl ToRedisArgs for Vote {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        let mut args = Vec::with_capacity(150);
        let choice = match self.choice {
            VoteOption::Yay => 0u8,
            VoteOption::Nay => 1u8,
            VoteOption::Abstain => 2u8,
        };
        let timestamp = self.timestamp.clone().to_be_bytes().to_vec();
        let voter = self.voter.as_bytes();
        let raw_byte_power = self.raw_byte_power.clone().to_be_bytes().to_vec();
        let worker_addr = self.worker_addr.as_bytes().to_vec();

        args.push(choice);
        for byte in timestamp {
            args.push(byte);
        }
        for byte in voter {
            args.push(byte);
        }
        for byte in raw_byte_power {
            args.push(byte);
        }
        for byte in worker_addr {
            args.push(byte);
        }

        args.write_redis_args(out);
    }
}

impl std::fmt::Display for Vote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let vote = match self.choice {
            VoteOption::Yay => "Yay",
            VoteOption::Nay => "Nay",
            VoteOption::Abstain => "Abstain",
        };
        let display_addr = self.worker_addr[0..8].to_string() + "..." + &self.worker_addr[self.worker_addr.len()-6..];
        write!(
            f,
            "{} voted {} at {} with {} byte power",
            display_addr, vote, self.timestamp, self.raw_byte_power
        )
    }
}

impl PartialEq for Vote {
    fn eq(&self, other: &Self) -> bool {
        self.voter == other.voter
    }
}

pub mod test_votes {
    use super::*;

    pub fn yay_vote() -> RecievedVote {
        RecievedVote {
            signature: "029273117441cea29c532c57612c132a84e28cdd372b4b12a8aba50f06da4469fa6b5534ab27a8c844aeee259e085ecaf706c50a2b6a1d5e439d08cadb714a105f838fd00873249539bd939dca5758a7cd42f82822c8ad0c7cb45a16275634e398".to_string(),
            worker_address: "t3qejyqmrirddrsb2w2thbaco3q6emuljumlhuonp3al35g3kkzx4zpeecycw7gim2meegemwot3gp3qr6alpa".to_string(),
            sp_id: "t06024".to_string()
        }
    }

    pub fn nay_vote() -> RecievedVote {
        RecievedVote {
            signature: "0293eafdcd619bd6ae1a86185fc6dbb2e534fba9086183cb9aa2c3f6feceb9441ecd9297981f1c1d23cffa1730535fc8411298e1650364ca666f4558240ab585af8556b07729b3c3c202fb5ac4477016510f744e768c0d0fce320613e70d64c006".to_string(),
            worker_address: "t3qejyqmrirddrsb2w2thbaco3q6emuljumlhuonp3al35g3kkzx4zpeecycw7gim2meegemwot3gp3qr6alpa".to_string(),
            sp_id: "t06024".to_string()
        }
    }

    pub fn abstain_vote() -> RecievedVote {
        RecievedVote {
            signature: "0295ce4b57f04994028b952c090b25a6f3979aa50b1604b91c25d769a18931b934380a303565e17ae1d8e2f3505b49d1fd120f5d3bd6ed6153d8fc13f988ea7453193ae67b84884bc5e537c55b45c8077ce8dd12fad3d09ecc62aa7f0695adff82".to_string(),
            worker_address: "t3qejyqmrirddrsb2w2thbaco3q6emuljumlhuonp3al35g3kkzx4zpeecycw7gim2meegemwot3gp3qr6alpa".to_string(),
            sp_id: "t06024".to_string()
        }
    }
}

#[cfg(test)]
mod votes_test {
    use redis::Value;

    use crate::votes::test_votes::*;

    use super::*;

    #[test]
    fn votes_pub_key() {
        let vote = yay_vote();

        let res = vote.pub_key();
        
        assert!(res.is_ok());
    }

    #[test]
    fn votes_sig() {
        let vote = yay_vote();

        let res = vote.sig();
        
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn votes_recover_vote() {
        let vote = yay_vote();

        let res = vote.recover_vote().await;

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Yay);
        assert_eq!(recovered_vote.worker_addr, vote.worker_address);

        let vote = nay_vote();

        let res = vote.recover_vote().await;

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Nay);
        assert_eq!(recovered_vote.worker_addr, vote.worker_address);

        let vote = abstain_vote();

        let res = vote.recover_vote().await;

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Abstain);
        assert_eq!(recovered_vote.worker_addr, vote.worker_address);
    }

    #[tokio::test]
    async fn votes_write_redis_args_vote() {
        let vote = yay_vote().recover_vote().await.unwrap();

        let mut args = Vec::new();
        vote.write_redis_args(&mut args);

        assert_eq!(args[0].len(), 159);
    }

    #[tokio::test]
    async fn votes_from_redis_value_vote() {
        let vote = yay_vote().recover_vote().await.unwrap();

        let mut args = Vec::new();
        vote.write_redis_args(&mut args);
        let value = Value::Data(args[0].clone());

        let res = Vote::from_redis_value(&value);

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Yay);
        assert_eq!(recovered_vote.worker_addr, vote.worker_addr);
    }
}