use std::time;

use redis::{from_redis_value, FromRedisValue, ToRedisArgs};
use serde::Deserialize;
use thiserror::Error;

use ic_verify_bls_signature::{PublicKey, Signature};

extern crate base64;
extern crate bls12_381 as bls;

use base64::{
    engine::general_purpose,
    Engine as _,
};

pub enum VoteOption {
    Yay,
    Nay,
    Abstain,
}

const YAY: VoteOption = VoteOption::Yay;
const NAY: VoteOption = VoteOption::Nay;
const ABSTAIN: VoteOption = VoteOption::Abstain;

const VOTE_OPTIONS: [u8; 3] = [YAY as u8, NAY as u8, ABSTAIN as u8];

#[derive(Debug, Error)]
pub enum VoteError {
    #[error("Could not recover vote choice from signature")]
    InvalidVoteOption,
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Invalid public key")]
    InvalidKey,
    #[error("Invalid base64 encoding")]
    InvalidBase64Encoding,
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

pub struct Vote {
    pub choice: VoteOption,
    timestamp: u64,
    voter: PublicKey,
    worker_addr: String,
}

#[derive(Deserialize)]
pub struct RecievedVote {
    signature: String,
    pk: String,
    worker_address: String,
}

impl RecievedVote {
    pub fn recover_vote(&self) -> Result<Vote, VoteError> {
        let pubk_bytes = match general_purpose::STANDARD.decode(&self.pk) {
            Ok(bytes) => bytes,
            Err(_) => return Err(VoteError::InvalidBase64Encoding),
        };

        let pubkey = match PublicKey::deserialize(&pubk_bytes) {
            Ok(pubkey) => pubkey,
            Err(_) => return Err(VoteError::InvalidKey),
        };

        let sig_bytes = match general_purpose::STANDARD.decode(&self.signature) {
            Ok(bytes) => bytes,
            Err(_) => return Err(VoteError::InvalidBase64Encoding),
        };

        let sig = match Signature::deserialize(&sig_bytes) {
            Ok(sig) => sig,
            Err(_) => return Err(VoteError::InvalidSignature),
        };

        for msg in VOTE_OPTIONS {
            match pubkey.verify(&[msg], &sig) {
                Ok(_) => {
                    return Ok(Vote {
                        choice: msg.into(),
                        timestamp: time::SystemTime::now()
                            .duration_since(time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        voter: pubkey,
                        worker_addr: self.worker_address.clone(),
                    });
                }
                Err(_) => (),
            }
        }

        return Err(VoteError::InvalidVoteOption);
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

        if args.len() != 73 {
            return Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Invalid vote",
            )));
        }

        let choice: VoteOption = args[0].into();

        let timestamp = u64::from_be_bytes([
            args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8],
        ]);

        let voter = match PublicKey::deserialize(&args[9..105]) {
            Ok(voter) => voter,
            Err(_) => {
                return Err(redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "Invalid voter key",
                )))
            }
        };

        let worker_addr = String::from_utf8(args[105..].to_vec()).unwrap();

        Ok(Vote {
            choice,
            timestamp,
            voter,
            worker_addr,
        })
    }
}

impl ToRedisArgs for Vote {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        let mut args = Vec::new();
        args.push(match self.choice {
            VoteOption::Yay => 0u8,
            VoteOption::Nay => 1u8,
            VoteOption::Abstain => 2u8,
        });
        args.extend_from_slice(&self.timestamp.to_be_bytes());
        args.extend_from_slice(&self.voter.serialize());
        args.extend_from_slice(&self.worker_addr.as_bytes());

        args.write_redis_args(out);
    }
}
