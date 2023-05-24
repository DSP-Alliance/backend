use std::time;

use hex::FromHexError;
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use redis::{from_redis_value, FromRedisValue, ToRedisArgs};
use serde::Deserialize;
use sha2::{
    digest::{core_api::CoreWrapper, generic_array::GenericArray},
    Digest,
};
use sha3::{Keccak256, Keccak256Core};
use thiserror::Error;

type Address = String;

pub enum VoteOption {
    Yay,
    Nay,
    Abstain,
}

const YAY: VoteOption = VoteOption::Yay;
const NAY: VoteOption = VoteOption::Nay;
const ABSTAIN: VoteOption = VoteOption::Abstain;

const VOTE_OPTIONS: [VoteOption; 3] = [YAY, NAY, ABSTAIN];

#[derive(Debug, Error)]
pub enum VoteError {
    #[error("Could not recover vote choice from signature")]
    InvalidVoteOption,
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Invalid recid")]
    InvalidRecid,
    #[error("Invalid hex")]
    InvalidHex(FromHexError),
}

impl VoteOption {
    fn digest(&self) -> CoreWrapper<Keccak256Core> {
        match self {
            VoteOption::Yay => Keccak256::new_with_prefix(b"0"),
            VoteOption::Nay => Keccak256::new_with_prefix(b"1"),
            VoteOption::Abstain => Keccak256::new_with_prefix(b"2"),
        }
    }
}

pub struct Vote {
    pub choice: VoteOption,
    timestamp: u64,
    voter: VerifyingKey,
}

#[derive(Deserialize)]
pub struct RecievedVote {
    signature: Address,
    recid: u8,
}

impl RecievedVote {
    pub fn recover_vote(&self) -> Result<Vote, VoteError> {
        let (sig, recid) = self.signature()?;

        let mut vote: Option<(VoteOption, VerifyingKey)> = None;
        for opt in VOTE_OPTIONS {
            let digest = opt.digest();
            match VerifyingKey::recover_from_digest(digest, &sig, recid) {
                Ok(key) => vote = Some((opt, key)),
                Err(_) => continue,
            };
        }

        match vote {
            Some(vote) => Ok(Vote {
                choice: vote.0,
                timestamp: time::SystemTime::now()
                    .duration_since(time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                voter: vote.1,
            }),
            None => Err(VoteError::InvalidVoteOption),
        }
    }

    fn signature(&self) -> Result<(Signature, RecoveryId), VoteError> {
        let sig_vec = hex::decode(&self.signature).map_err(|e| VoteError::InvalidHex(e))?;
        if sig_vec.len() != 64 {
            return Err(VoteError::InvalidSignature);
        }
        let sig = GenericArray::from_slice(&sig_vec);
        let recid = match RecoveryId::from_byte(self.recid) {
            Some(recid) => recid,
            None => return Err(VoteError::InvalidRecid),
        };

        Ok((Signature::from_bytes(&sig).unwrap(), recid))
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

        let choice = match &args[0] {
            0 => YAY,
            1 => NAY,
            2 => ABSTAIN,
            _ => return Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Unknown vote option",
            ))),
        };

        let timestamp = u64::from_be_bytes([args[1], args[2], args[3], args[4], args[5], args[6], args[7], args[8]]);

        let voter = match VerifyingKey::from_sec1_bytes(&args[9..73]) {
            Ok(voter) => voter,
            Err(_) => return Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Invalid voter key",
            ))),
        };
        
        Ok(Vote {
            choice,
            timestamp,
            voter,
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
        args.extend_from_slice(&self.voter.to_sec1_bytes());

        args.write_redis_args(out);
    }
}
