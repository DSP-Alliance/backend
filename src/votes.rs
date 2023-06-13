use std::str::FromStr;

use ethers::{types::Address, prelude::*};
use redis::{from_redis_value, FromRedisValue, ToRedisArgs};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, PartialEq, Clone)]
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

        Ok(Vote { choice, address, fip })
    }
    fn msg_details(&self) -> Result<(VoteOption, u32), VoteError> {
        let msg: Vec<String> = self.message.split_whitespace().map(|s| s.to_string()).collect();

        let (choice, fip_str) = match msg.as_slice() {
            [choice, fip] => (choice, fip),
            _ => return Err(VoteError::InvalidMessageFormat),
        };

        let choice = match choice.as_str() {
            "YAY" => Ok(VoteOption::Yay),
            "NAY" => Ok(VoteOption::Nay),
            "ABSTAIN" => Ok(VoteOption::Abstain),
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
        let message_hash = ethers::utils::keccak256(&self.message.as_bytes());

        let address = signature.recover(message_hash)?;

        Ok(address)
    }
}

impl Vote {
    pub fn choice(&self) -> VoteOption {
        self.choice.clone()
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

impl Into<u8> for VoteOption {
    fn into(self) -> u8 {
        match self {
            VoteOption::Yay => 0,
            VoteOption::Nay => 1,
            VoteOption::Abstain => 2,
        }
    }
}

impl VoteOption {
    pub fn to_bytes(&self, fip_number: u32) -> [u8; 5] {
        let mut bytes = [0u8; 5];
        bytes[0] = match self {
            VoteOption::Yay => 0u8,
            VoteOption::Nay => 1u8,
            VoteOption::Abstain => 2u8,
        };
        bytes[1..5].copy_from_slice(&fip_number.to_be_bytes());
        bytes
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
        let fip = self.fip.clone().to_be_bytes().to_vec();
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
        write!(
            f,
            "{} voted {} on FIP-{}",
            self.address, vote, self.fip
        )
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
                vote.signature = "0298d866dd504c531d56fc5e7b845cd907e47067883102e1165b1921b692f8b4434f8852e89dd92da202eb8713e5c1a51a0f48721517bf7b028c6a04e8adeaf9b9fd26808bc8035fcc6a558b8bef707d960d9b88c248f0da6a9f4e56d75e2a7acb".to_string();
                vote
            }
            2 => {
                vote.signature = "02a51f582d42caca616d35cffebf16f1f1507593096f12d8542292d6c9c076293de6f1f7f61bcb2d705adde42e6ec87f0c012a698a48e02c1c0e6f1ad0754410542fc4f0a61faa5937fa6d0e3f321b5eb3bf234f311a6ae730fc9a669b3f4d91af".to_string();
                vote
            }
            3 => {
                vote.signature = "0290cf7d5c7fdd7a6168ae60b0b0d484e14bc75f27d8b8b79302958329f7b5a9738a2fc158410ce2affb7aacbd32ab9321116cef0e67a53a5f038696e58a62ff410800a6fc8ba957f1b4e42c6396df63cf9df3f892220f6d53c7077c5da98a59f7".to_string();
                vote
            }
            4 => {
                vote.signature = "028d6e34f3749e2d95323304df811e1a2fbb6421373efca92803cdbd970aaf1de8faa6696723a2599ea136936335ccd9ba0ff848732437aab91cc2b5a0a7c978e20eb1a1bb6b741c229b5dd71d2039f1cec9acfbd717c5136a8096d15562e09b8a".to_string();
                vote
            }
            5 => {
                vote.signature = "02872f47a9119353573fd941d1e62f8d0aa4eea53c5ac34cb174e0eaae1a28d21d9fd82b0ea1b2aa6cf2edc065a4d5aadf10332f45da62886a44df0dec0bf93893171883c3e6e686e81a5c739f258362bdbc5364ec1c14e9943fa640afb8ebb7dc".to_string();
                vote
            }
            _ => panic!("Invalid vote number"),
        }
    }

    fn nay(num: u32) -> ReceivedVote {
        let mut vote = ReceivedVote::default();
        match num {
            1 => {
                vote.signature = "02b1e5489c856a02506e7660cebc813cb94919e7aa42e595fa00276552b09832dd0bacf2b79aefe5da89ed0ecfff92fb1701c4a314c0274b62979cc673bc4ba2bdce7b0fb07944c0257e3274a86f09acfb07f1fa54b96b80282bb309033fd77cdd".to_string();
                vote
            }
            2 => {
                vote.signature = "02b9023540cfd8d457bf5bd7d7aae834c13823e6ee24db1d9b6c3898b50083e6bd381e112e4ab8ac1f2d17e24d723f1e480da308387938f12177e90360ef884d869dcdf28df572acf286d9f43a9cab22ce7200d6b9b2ed54d6967d1baee7136e8a".to_string();
                vote
            }
            3 => {
                vote.signature = "028afec6fe8a9a0d1c654f041929dabc9bcbf9ae4935f429e323b5539316f61fa75f4b209832b46d5b28b7d4b9e482e249020bebab3a1e2c43742db0f4d140fa0070296df13264715abaf6c573bf4410959df1b91e17090403a9d59ac18484d3f8".to_string();
                vote
            }
            4 => {
                vote.signature = "02b75741b50aa2a625db554eb472b951bd1bb99537e64b64f662d642496d7ef05b32fcda990695e13ec0e3aa61dbbae82f074c0c4c70c96de99b1765b7208f4b17c5e70c4406009dc9e682196b4cd5d854a03b0f74fad437b923263b8d44b610b0".to_string();
                vote
            }
            5 => {
                vote.signature = "028e5e0b0b3b07073103bea46e5829540b5d64cb6b07b32c8718ce8d2f1e006ab16ac5b1a0f4bf7ebb60db339cf246f236018cafedf9808a2f89df15b92c7d7754cd12302eb35be56f15c5741bf51435dd09d59a00e87685f6b393186d0b170e78".to_string();
                vote
            }
            _ => panic!("Invalid vote number"),
        }
    }

    fn abstain(num: u32) -> ReceivedVote {
        let mut vote = ReceivedVote::default();
        match num {
            1 => {
                vote.signature = "02acda264e19097ba93e1c4741d070e8eb71b1d18b3833026da648b8cf1edf7d014d2af097959d33923250ccaa3ab001e9147daa6c0f1a99382602fdefede31843f728e602eb18eda67119649f5e94f50e26075e4db22962c8785d4b1d076afa2b".to_string();
                vote
            }
            2 => {
                vote.signature = "02a553d1c64bdd696f01ee8309a1395b0ec2a81fce852b95fa36c397c1590bc6c8d79ce86d76dec67672439bf713dd3cff07162ac226e082484971fe4e525bf129d8d82882f538337bae7b40741ba31d8390ee3701737497ee75c316a4274dcbd1".to_string();
                vote
            }
            3 => {
                vote.signature = "028e07a39943b4cc14b9f14f5ccf05afde74b815c2632c0d02fa2119cc171f97417ced06fdfd7f39f3909a90bab4caf30607f4182c04306f81b05ed2d3a3532108facd2c3302608dc607d7aa243acd68de60b0a68cfb9d17db0caba5e22d79b315".to_string();
                vote
            }
            4 => {
                vote.signature = "02a66fab87bb230e592db01857e49818d4ea68b1901dfe01c8eb65ad46ac4cc5ff5c369d5f9d028f8e5865dd29c7a3078201e9730d18129a27e45b2c3f182ed5857c2a2eb463ad5210983d4af2ee362fa6fddf9d28e52eacb2987f5ecf6b9829cb".to_string();
                vote
            }
            5 => {
                vote.signature = "02ab6b37b21b41c0d29249c23d1821484bc6540bc82878bfb55f13750a7dc542295aa2096fe286b1fe4b246f9653097d7b001c8e89e7af7bfc970e49a3611d5e4eef2c2b9fc1f37e92c332dc77f9ba86cad30c360e1b9433adf55e1fa54f68743d".to_string();
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
/*
#[cfg(test)]
mod votes_test {
    use redis::Value;

    use crate::votes::test_votes::test_vote;

    use super::*;

    #[test]
    fn votes_vote_option_to_bytes() {
        let yay = VoteOption::Yay;
        let nay = VoteOption::Nay;
        let abstain = VoteOption::Abstain;

        assert_eq!(yay.to_bytes(1423), [0, 0, 0, 5, 143]);
        assert_eq!(nay.to_bytes(1423), [1, 0, 0, 5, 143]);
        assert_eq!(abstain.to_bytes(1423), [2, 0, 0, 5, 143]);
    }

    #[test]
    fn votes_pub_key() {
        let vote = test_vote(VoteOption::Yay, 1u32);

        let res = vote.pub_key();

        assert!(res.is_ok());
    }

    #[test]
    fn votes_sig() {
        let vote = test_vote(VoteOption::Yay, 1u32);

        let res = vote.sig();

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn votes_recover_vote() {
        let vote = test_vote(VoteOption::Yay, 1u32);

        let res = vote.recover_vote(1u32).await;

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Yay);
        assert_eq!(recovered_vote.worker_addr, vote.worker_address);

        let vote = test_vote(VoteOption::Nay, 1u32);

        let res = vote.recover_vote(1u32).await;

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Nay);
        assert_eq!(recovered_vote.worker_addr, vote.worker_address);

        let vote = test_vote(VoteOption::Abstain, 1u32);

        let res = vote.recover_vote(1u32).await;

        assert!(res.is_ok());

        let recovered_vote = res.unwrap();

        assert_eq!(recovered_vote.choice, VoteOption::Abstain);
        assert_eq!(recovered_vote.worker_addr, vote.worker_address);
    }

    #[tokio::test]
    async fn votes_write_redis_args_vote() {
        let vote = test_vote(VoteOption::Yay, 1u32)
            .recover_vote(1u32)
            .await
            .unwrap();

        let mut args = Vec::new();
        vote.write_redis_args(&mut args);

        assert_eq!(args[0].len(), 159);
    }

    #[tokio::test]
    async fn votes_from_redis_value_vote() {
        let vote = test_vote(VoteOption::Yay, 1u32)
            .recover_vote(1u32)
            .await
            .unwrap();

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
*/