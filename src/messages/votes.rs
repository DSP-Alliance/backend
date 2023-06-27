use std::str::FromStr;

use ethers::{prelude::*, types::Address};
use redis::{from_redis_value, FromRedisValue, ToRedisArgs};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum VoteOption {
    Yay,
    Nay,
    Abstain,
}

#[derive(Debug, Error)]
pub enum VoteError {
    #[error(transparent)]
    SignatureError(#[from] SignatureError),
    #[error("Invalid message format")]
    InvalidMessageFormat,
    #[error("Invalid vote option")]
    InvalidVoteOption,
}

#[derive(Serialize, Deserialize)]
pub struct Vote {
    choice: VoteOption,
    address: Address,
    fip: u32,
}

/// Message scheme
///
/// YAY: FIP-xxx
#[derive(Deserialize, Default)]
pub struct ReceivedVote {
    signature: String,
    message: String,
}

impl ReceivedVote {
    pub fn vote(&self) -> Result<Vote, VoteError> {
        let (choice, fip) = self.msg_details()?;
        let address = self.pub_key()?;

        Ok(Vote {
            choice,
            address,
            fip,
        })
    }
    fn msg_details(&self) -> Result<(VoteOption, u32), VoteError> {
        let msg: Vec<String> = self
            .message
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        let (choice, fip_str) = match msg.as_slice() {
            [choice, fip] => (choice, fip),
            _ => return Err(VoteError::InvalidMessageFormat),
        };

        let choice = match choice.as_str() {
            "YAY:" => Ok(VoteOption::Yay),
            "NAY:" => Ok(VoteOption::Nay),
            "ABSTAIN:" => Ok(VoteOption::Abstain),
            _ => Err(VoteError::InvalidVoteOption),
        }?;

        let fip = fip_str
            .strip_prefix("FIP-")
            .ok_or(VoteError::InvalidMessageFormat)?
            .parse::<u32>()
            .map_err(|_| VoteError::InvalidMessageFormat)?;

        Ok((choice, fip))
    }
    fn pub_key(&self) -> Result<Address, VoteError> {
        let signature = Signature::from_str(&self.signature)?;
        let msg = format!(
            "\x19Ethereum Signed Message:\n{}{}",
            self.message.len(),
            self.message
        );
        let message_hash = ethers::utils::keccak256(msg);

        let address = signature.recover(message_hash)?;

        Ok(address)
    }
}

impl Vote {
    pub fn choice(&self) -> VoteOption {
        self.choice.clone()
    }

    pub fn voter(&self) -> Address {
        self.address
    }
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

impl From<VoteOption> for u8 {
    fn from(vote: VoteOption) -> Self {
        match vote {
            VoteOption::Yay => 0,
            VoteOption::Nay => 1,
            VoteOption::Abstain => 2,
        }
    }
}

impl FromRedisValue for VoteOption {
    fn from_redis_value(v: &redis::Value) -> redis::RedisResult<Self> {
        let s: u8 = from_redis_value(v)?;
        match s {
            0 => Ok(VoteOption::Yay),
            1 => Ok(VoteOption::Nay),
            2 => Ok(VoteOption::Abstain),
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
        if args.len() != 25 {
            return Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Invalid vote format",
            )));
        }

        let choice: VoteOption = args[0].into();

        let address = Address::from_slice(&args[1..21]);

        let fip = u32::from_be_bytes(args[21..25].try_into().unwrap());

        Ok(Vote {
            choice,
            address,
            fip,
        })
    }
}

impl ToRedisArgs for Vote {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        let mut args = Vec::with_capacity(25);
        let choice: u8 = self.choice.clone().into();
        let fip = self.fip.to_be_bytes().to_vec();
        let addr = self.address.as_fixed_bytes().to_vec();

        args.push(choice);
        for byte in addr {
            args.push(byte);
        }
        for byte in fip {
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
        write!(f, "{} voted {} on FIP-{}", self.address, vote, self.fip)
    }
}

impl PartialEq for Vote {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address && self.fip == other.fip
    }
}

pub mod test_votes {
    use super::*;

    fn yay(num: u32) -> ReceivedVote {
        let mut vote = ReceivedVote::default();
        match num {
            1 => {
                vote.signature = "0x67ae6539cd110b9a043e3836303771d8a8ec13c7c688f369cc1a8a9f997128bf207319c7e94a60f9739c51510cb483c8f0c2efa32147690ae8221c08d34352ec1b".to_string();
                vote.message = "YAY: FIP-1".to_string();
                vote
            }
            2 => {
                vote.signature = "0xc5e380ff34eda4c028e100ad808c42a0d304a31d27f40a32ec4ecc864df29fac00b130de1ef08e362a89c3349d2e00eb793fe1e3c4b4fb6a676e393646caf3291b".to_string();
                vote.message = "YAY: FIP-2".to_string();
                vote
            }
            3 => {
                vote.signature = "0x4080a0f4a2a137827af25ac40c7891a42fe92f05385eecc6d7c2bf0bfbbd2552525215c7ccd32f97b7b1a5a22bd812ca2277642b4c038e8aab9b3801fae400521c".to_string();
                vote.message = "YAY: FIP-3".to_string();
                vote
            }
            4 => {
                vote.signature = "0x6ca909939f925a502e4311b4b351276e534e93b09ec710dbf817df54c3fb6e1a782fd88091facdea51138fb3a2282ab01b8caa54f3ba486827add40bb953b3b11c".to_string();
                vote.message = "YAY: FIP-4".to_string();
                vote
            }
            5 => {
                vote.signature = "0x6aded16a76903fc3a957ee0704a3599bb577b848381930c4bf06c5dd5e4cb87a39f8fe07f80bee085188a2e4e151dacac45f63b1da891d6d0bf7337a6a3834571b".to_string();
                vote.message = "YAY: FIP-5".to_string();
                vote
            }
            _ => panic!("Invalid vote number"),
        }
    }

    fn nay(num: u32) -> ReceivedVote {
        let mut vote = ReceivedVote::default();
        match num {
            1 => {
                vote.signature = "0x66b3478b3ff31992bdc6c9755bca8a3a40975fddf96f884981ca596a33186f677eef3b8d97dbae93602ae7e7d1c6b7d25efb08105fde5286fd4d6f45dc2558f81c".to_string();
                vote.message = "NAY: FIP-1".to_string();
                vote
            }
            2 => {
                vote.signature = "0x84c865fc1e1cc9b8fe582867143501da107a70a7bd4a9c1d32dec8acd1832c634c1ae6475ee7f525d18a361d8b9bfd15ec8e6c32f8dc83980b8e7d73c0983f931b".to_string();
                vote.message = "NAY: FIP-2".to_string();
                vote
            }
            3 => {
                vote.signature = "0xcf1667e86a3be597aba7aa9a513862c3321e311e62b70705ab21e1637c016efc43664ee5f130f3815cbcf89ee50fb71435848299f6e72f5b8258bad15a81f1f91c".to_string();
                vote.message = "NAY: FIP-3".to_string();
                vote
            }
            4 => {
                vote.signature = "0x7e8b227fdd90b14e88b95060955d454bec53d3953e01d65682e8457f85706235680af102dacc9a5ddea05017255a6c8a3b52ebb1f75ab5fb8b7f54d71701fdac1c".to_string();
                vote.message = "NAY: FIP-4".to_string();
                vote
            }
            5 => {
                vote.signature = "0x3ad40ddaa8198e0d114e1551a629bf3b5dc84f31d9245cf9da483c073f8d21f405f1650e86e857bf7c7ad08c3e5b43086066fd8efb54be23f739efd0d7ee98971b".to_string();
                vote.message = "NAY: FIP-5".to_string();
                vote
            }
            _ => panic!("Invalid vote number"),
        }
    }

    fn abstain(num: u32) -> ReceivedVote {
        let mut vote = ReceivedVote::default();
        match num {
            1 => {
                vote.signature = "0x4bbe43a386b903e730896ce976ca2e02dfa32e1208399f028554d121301ccca96b8fba474343e422bc949aa5b6d662ad5026dd8b04cfe58847b6d4f3d500b3e81b".to_string();
                vote.message = "ABSTAIN: FIP-1".to_string();
                vote
            }
            2 => {
                vote.signature = "0x27d5186976e4c23b4d7d2e4fb5a7d5118b3f838d45fdc0e8e47682fbf6d2d8df0a25f2704cfe2845f9cc1eba982db7c14751bc61a9784b27f7968a6def56f1de1b".to_string();
                vote.message = "ABSTAIN: FIP-2".to_string();
                vote
            }
            3 => {
                vote.signature = "0x8783fb87eab644b324e3e7361139448bd3a7826103c6f8eff7f6ca8141af92ee7d297529f52e795a76f37a7628d54e8bed5b0e91213994083c4666449c3d594b1c".to_string();
                vote.message = "ABSTAIN: FIP-3".to_string();
                vote
            }
            4 => {
                vote.signature = "0xe7a637ed491d8257716342ad10b2655da54f8f5c3abe16ef104d3701e4c4e1b24da8486bf5d728c33a3450c2ce4cae69a1c2cb9a4c2f1f762c0ca99c307f8a921b".to_string();
                vote.message = "ABSTAIN: FIP-4".to_string();
                vote
            }
            5 => {
                vote.signature = "0xc0dfd2291c2ba5223937b9af815c8a6f1042ecfa046a63471a74354055b871b22d802192d8409d4b974fe462fcf78e99120f344dfb3e728b5cf339e6210cb7d91c".to_string();
                vote.message = "ABSTAIN: FIP-5".to_string();
                vote
            }
            _ => panic!("Invalid vote number"),
        }
    }

    pub fn test_vote(choice: VoteOption, num: u32) -> ReceivedVote {
        match choice {
            VoteOption::Yay => yay(num),
            VoteOption::Nay => nay(num),
            VoteOption::Abstain => abstain(num),
        }
    }
}

#[cfg(test)]
mod votes_test {
    use redis::Value;

    use super::test_votes::test_vote;

    use super::*;

    #[test]
    fn votes_pub_key() {
        let vote = test_vote(VoteOption::Yay, 1u32);

        let res = vote.pub_key();

        assert!(res.is_ok());

        let res = vote.msg_details();

        assert!(res.is_ok());
    }

    #[test]
    fn votes_msg_details() {
        let options = vec![VoteOption::Yay, VoteOption::Nay, VoteOption::Abstain];
        let fip_nums = 1..=5;
        for option in options {
            for num in fip_nums.clone() {
                let vote = test_vote(option.clone(), num);

                let res = vote.msg_details();

                assert!(res.is_ok());

                let (option1, fip) = res.unwrap();

                assert_eq!(option1, option);
                assert_eq!(fip, num);
            }
        }
    }
    #[tokio::test]
    async fn votes_recover_vote() {
        let real_addr = Address::from_str("0xf2361d2a9a0677e8ffd1515d65cf5190ea20eb56").unwrap();

        let vote = test_vote(VoteOption::Yay, 1u32);

        let res = vote.vote();

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Yay);
        assert_eq!(recovered_vote.address, real_addr);
        assert_eq!(recovered_vote.fip, 1u32);

        println!("{:?}", recovered_vote.address);

        let vote = test_vote(VoteOption::Nay, 1u32);

        let res = vote.vote();

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Nay);
        assert_eq!(recovered_vote.address, real_addr);
        assert_eq!(recovered_vote.fip, 1u32);

        let vote = test_vote(VoteOption::Abstain, 1u32);

        let res = vote.vote();

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Abstain);
        assert_eq!(recovered_vote.address, real_addr);
        assert_eq!(recovered_vote.fip, 1u32);
    }

    #[tokio::test]
    async fn votes_write_redis_args_vote() {
        let vote = test_vote(VoteOption::Yay, 1u32).vote().unwrap();

        let mut args = Vec::new();
        vote.write_redis_args(&mut args);

        assert_eq!(args[0].len(), 25);
    }

    #[tokio::test]
    async fn votes_from_redis_value_vote() {
        let real_addr = Address::from_str("0xf2361d2a9a0677e8ffd1515d65cf5190ea20eb56").unwrap();
        let vote = test_vote(VoteOption::Yay, 1u32).vote().unwrap();

        let mut args = Vec::new();
        vote.write_redis_args(&mut args);
        let value = Value::Data(args[0].clone());

        let res = Vote::from_redis_value(&value);

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Yay);
        assert_eq!(recovered_vote.address, real_addr);
        assert_eq!(recovered_vote.fip, 1u32);
    }
}
