extern crate redis;

use std::{mem::MaybeUninit, time};

use redis::{Commands, Connection, RedisError};
use serde::Serialize;
use url::Url;

use crate::votes::{Vote, VoteOption};

pub struct Redis {
    con: Connection,
}

#[derive(Debug, PartialEq)]
pub enum VoteStatus {
    DoesNotExist,
    InProgress,
    Concluded,
}

enum LookupKey {
    FipNumber(u32),
    Timestamp(u32),
}

impl LookupKey {
    fn to_bytes(&self) -> Vec<u8> {
        let (lookup_type, fip) = match self {
            LookupKey::FipNumber(fip) => (0, fip),
            LookupKey::Timestamp(fip) => (1, fip),
        };
        let slice = unsafe {
            let mut key = MaybeUninit::<[u8; 5]>::uninit();
            let start = key.as_mut_ptr() as *mut u8;
            (start.add(0) as *mut [u8; 4]).write(fip.to_be_bytes());

            // This is the bit we set to 0 if we only want the token object
            (start.add(4) as *mut [u8; 1]).write([lookup_type as u8]);

            key.assume_init()
        };
        Vec::from(slice)
    }
}

#[derive(Serialize)]
struct VoteResults {
    yay: u64,
    nay: u64,
    abstain: u64,
    yay_storage_size: u128,
    nay_storage_size: u128,
    abstain_storage_size: u128,
}

/*
   TODO: Set up table for tracking storage size of votes
*/

impl Redis {
    pub fn new(path: impl Into<Url>) -> Result<Redis, RedisError> {
        let client = redis::Client::open(path.into())?;
        let con = client.get_connection()?;

        Ok(Self { con })
    }

    /*~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~/
    /                                 INITIALIZATION                                 /
    /~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~*/

    /// This function assumes that the FIP number is not already in the database
    pub fn new_vote(
        &mut self,
        fip_number: impl Into<u32>,
        vote: Option<Vote>,
    ) -> Result<(), RedisError> {
        // If vote is None, set the vector to empty
        let vote = match vote {
            Some(v) => vec![v],
            None => vec![],
        };

        let fip_num = fip_number.into();

        let vote_key = LookupKey::FipNumber(fip_num).to_bytes();
        let time_key = LookupKey::Timestamp(fip_num).to_bytes();

        // Set a map of FIP number to vector of all votes
        self.con.set::<Vec<u8>, Vec<Vote>, ()>(vote_key, vote)?;

        // Set a map of FIP to timestamp of vote start
        let timestamp = time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.con.set::<Vec<u8>, u64, ()>(time_key, timestamp)?;

        Ok(())
    }

    /*~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~/
    /                                     GETTERS                                    /
    /~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~*/

    pub fn vote_results(&mut self, fip_number: impl Into<u32>) -> Result<String, RedisError> {
        let mut yay = 0;
        let mut nay = 0;
        let mut abstain = 0;

        let votes = self.votes(fip_number)?;

        for vote in votes {
            match vote.choice {
                VoteOption::Yay => yay += 1,
                VoteOption::Nay => nay += 1,
                VoteOption::Abstain => abstain += 1,
            }
        }

        let results = VoteResults {
            yay,
            nay,
            abstain,
            yay_storage_size: 0,
            nay_storage_size: 0,
            abstain_storage_size: 0,
        };

        match serde_json::to_string(&results) {
            Ok(j) => Ok(j),
            Err(_) => Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Error serializing vote results",
            ))),
        }
    }

    pub fn vote_status(&mut self, fip_number: impl Into<u32>, vote_length: impl Into<u64>) -> Result<VoteStatus, RedisError> {
        let num = fip_number.into();
        let vote_key = LookupKey::FipNumber(num).to_bytes();
        let time_key = LookupKey::Timestamp(num).to_bytes();

        // Check if the FIP number exists in the database
        if !self.con.exists(vote_key)? {
            return Ok(VoteStatus::DoesNotExist);
        }

        // Check if the FIP number has a timestamp
        if !self.con.exists(time_key.clone())? {
            return Ok(VoteStatus::DoesNotExist);
        }

        // Check if the vote is still open
        let time_start: u64 = self.vote_start(num)?;
        let now = time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now - time_start < vote_length.into() {
            return Ok(VoteStatus::InProgress);
        } else {
            return Ok(VoteStatus::Concluded);
        }
    }

    fn vote_start(&mut self, fip_number: impl Into<u32>) -> Result<u64, RedisError> {
        let key = LookupKey::Timestamp(fip_number.into()).to_bytes();
        let timestamp: u64 = self.con.get::<Vec<u8>, u64>(key)?;
        Ok(timestamp)
    }

    fn votes(&mut self, fip_number: impl Into<u32>) -> Result<Vec<Vote>, RedisError> {
        let key = LookupKey::FipNumber(fip_number.into()).to_bytes();
        let votes: Vec<Vote> = self.con.get::<Vec<u8>, Vec<Vote>>(key)?;
        Ok(votes)
    }

    /*~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~/
    /                                     SETTERS                                    /
    /~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~*/

    pub fn add_vote<T>(&mut self, fip_number: T, vote: Vote) -> Result<(), RedisError>
    where
        T: Into<u32>,
    {
        let num: u32 = fip_number.into();

        if self.votes(num)?.is_empty() {
            self.new_vote(num, Some(vote))?;
            return Ok(());
        }

        let key = LookupKey::FipNumber(num.into()).to_bytes();

        let mut votes: Vec<Vote> = self.con.get::<Vec<u8>, Vec<Vote>>(key.clone())?;

        if votes.contains(&vote) {
            return Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Vote already exists",
            )));
        }
        votes.push(vote);
        self.con.set::<Vec<u8>, Vec<Vote>, ()>(key.clone(), votes)?;
        println!("set votes");
        Ok(())
    }

    pub fn flush_vote(&mut self, fip_number: impl Into<u32>) -> Result<(), RedisError> {
        let key = LookupKey::FipNumber(fip_number.into()).to_bytes();
        self.con.del::<Vec<u8>, ()>(key)?;
        Ok(())
    }

    pub fn flush_all_votes(&mut self) -> Result<(), RedisError> {
        let keys: Vec<Vec<u8>> = self.con.keys("*")?;
        for key in keys {
            self.con.del::<Vec<u8>, ()>(key)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::votes::test_votes::*;

    fn redis() -> Redis {
        let url = Url::parse("redis://127.0.0.1:6379").unwrap();
        let mut redis = Redis::new(url).unwrap();

        for i in 1..=10 {
            redis.flush_vote(i as u32).unwrap();
        }

        redis
    }

    #[tokio::test]
    async fn redis_votes() {
        let mut redis = redis();

        let res = redis.votes(5u32);

        match res {
            Ok(_) => {},
            Err(e) => panic!("Error: {}", e),
        }

        // let votes = res.unwrap();
        // for v in votes {
        //     println!("{}", v);
        // }
    }

    #[tokio::test]
    async fn redis_vote_start() {
        let mut redis = redis();

        let vote = yay_vote().recover_vote().await.unwrap();
        assert!(redis.add_vote(4u32, vote).is_ok());

        let res = redis.vote_start(4u32);

        match res {
            Ok(_) => {},
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn redis_vote_status() {
        let mut redis = redis();

        let vote = yay_vote().recover_vote().await.unwrap();
        assert!(redis.add_vote(3u32, vote).is_ok());


        let vote_start = redis.vote_start(3u32).unwrap();

        tokio::time::sleep(time::Duration::from_secs(2)).await;

        let time_now = time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let ongoing = time_now - vote_start + 1;
        let concluded = time_now - vote_start - 1;

        let res = redis.vote_status(3u32, ongoing);

        match res {
            Ok(_) => {},
            Err(e) => panic!("Error: {}", e),
        }
        assert_eq!(res.unwrap(), VoteStatus::InProgress);

        let res = redis.vote_status(3u32, concluded);

        match res {
            Ok(_) => {},
            Err(e) => panic!("Error: {}", e),
        }
        assert_eq!(res.unwrap(), VoteStatus::Concluded);

        let res = redis.vote_status(1234089398u32, concluded);

        match res {
            Ok(_) => {},
            Err(e) => panic!("Error: {}", e),
        }
        assert_eq!(res.unwrap(), VoteStatus::DoesNotExist);
    }

    #[tokio::test]
    async fn redis_add_vote() {
        let mut redis = redis();

        let vote = yay_vote().recover_vote().await.unwrap();

        let res = redis.add_vote(2u32, vote);

        match res {
            Ok(_) => {},
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn redis_vote_results() {
        let mut redis = redis();
        let vote = yay_vote().recover_vote().await.unwrap();

        let res = redis.add_vote(1u32, vote);
        println!("{:?}", res);
        assert!(res.is_ok());

        let res = redis.vote_results(1u32);

        match res {
            Ok(_) => {},
            Err(e) => panic!("Error: {}", e),
        }
    }
}
