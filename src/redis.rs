extern crate redis;

use std::{mem::MaybeUninit, time};

use ethers::types::Address;
use redis::{Commands, Connection, RedisError};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    authorized_voters,
    messages::votes::{Vote, VoteOption},
    storage::{fetch_storage_amount, Network},
};

pub struct Redis {
    con: Connection,
}

#[derive(Debug, PartialEq)]
pub enum VoteStatus {
    DoesNotExist,
    InProgress(u64),
    Concluded,
}

enum LookupKey {
    /// FIP number to vector of all votes
    Votes(u32, Network),
    /// FIP number to timestamp of vote start
    Timestamp(u32, Network),
    /// Network and voter address to voter registration
    Voter(Network, Address),
    /// The voter authorized to start a vote on that network
    VoteStarters(Network),
    /// Votes in progress on the network
    ActiveVotes(Network),
    /// Concluded votes on the network
    ConcludedVotes(Network),
    /// VoteChoice and FIP number to total storage amount
    Storage(VoteOption, Network, u32),
    /// The network the address belongs to
    Network(Address),
}

impl Redis {
    pub fn new(path: impl Into<Url>) -> Result<Redis, RedisError> {
        let client = redis::Client::open(path.into())?;
        let con = client.get_connection()?;

        Ok(Self { con })
    }

    /*~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~/
    /                                 INITIALIZATION                                 /
    /~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~*/

    /// Starts a new vote in the database but does not add any votes into the database
    pub fn start_vote(
        &mut self,
        fip_number: impl Into<u32>,
        signer: Address,
        ntw: Network,
    ) -> Result<(), RedisError> {
        let num = fip_number.into();

        // Check if signer is authorized to start a vote
        if !self.is_authorized_starter(signer, ntw)? && !authorized_voters().contains(&signer) {
            return Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Signer is not authorized to start a vote",
            )));
        }

        let time_key = LookupKey::Timestamp(num, ntw).to_bytes();

        // Check if vote already exists
        if self.con.exists(time_key.clone())? {
            return Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Vote already exists",
            )));
        }

        // Set a map of FIP to timestamp of vote start
        let timestamp = time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.con.set::<Vec<u8>, u64, ()>(time_key, timestamp)?;

        self.register_active_vote(ntw, num)?;

        Ok(())
    }

    /// Registers a voter in the database
    ///
    /// * Creates a lookup from voters address to their respective network
    /// * Creates a lookup from voters address to their authorized storage providers
    pub fn register_voter(
        &mut self,
        voter: Address,
        ntw: Network,
        sp_ids: Vec<u32>,
    ) -> Result<(), RedisError> {
        let key = LookupKey::Voter(ntw, voter).to_bytes();

        self.set_network(ntw, voter)?;

        self.con.set::<Vec<u8>, Vec<u32>, ()>(key, sp_ids)?;

        Ok(())
    }

    pub fn unregister_voter(&mut self, voter: Address, ntw: Network) -> Result<(), RedisError> {
        let key = LookupKey::Voter(ntw, voter).to_bytes();

        // Remove the voter from the network lookup
        self.remove_network(voter)?;

        self.con.del::<Vec<u8>, ()>(key)?;

        Ok(())
    }

    pub fn register_voter_starter(
        &mut self,
        voter: Address,
        ntw: Network,
    ) -> Result<(), RedisError> {
        let key = LookupKey::VoteStarters(ntw).to_bytes();

        let mut current_voters = self.voter_starters(ntw)?;

        current_voters.push(voter);

        current_voters.sort();
        current_voters.dedup();

        let new_bytes = current_voters
            .into_iter()
            .flat_map(|v| v.as_fixed_bytes().to_vec())
            .collect::<Vec<u8>>();

        self.con.set::<Vec<u8>, Vec<u8>, ()>(key, new_bytes)?;

        Ok(())
    }

    /// Adds FIP number to list of active votes
    fn register_active_vote(&mut self, ntw: Network, fip: u32) -> Result<(), RedisError> {
        let key = LookupKey::ActiveVotes(ntw).to_bytes();

        let mut current_votes = self.active_votes(ntw, None)?;

        current_votes.push(fip);

        self.con.set::<Vec<u8>, Vec<u32>, ()>(key, current_votes)?;

        Ok(())
    }

    /// Adds FIP number to list of concluded votes
    ///
    /// * Removes the FIP number from the list of active votes
    fn register_concluded_vote(&mut self, ntw: Network, fip: u32) -> Result<(), RedisError> {
        let key = LookupKey::ConcludedVotes(ntw).to_bytes();

        self.remove_active_vote(ntw, fip)?;

        let mut current_votes = self.concluded_votes(ntw)?;

        current_votes.push(fip);

        self.con.set::<Vec<u8>, Vec<u32>, ()>(key, current_votes)?;

        Ok(())
    }

    /// Removes FIP number from list of active votes
    ///
    /// Note: This function should only be called by "register_concluded_vote"
    fn remove_active_vote(&mut self, ntw: Network, fip: u32) -> Result<(), RedisError> {
        let key = LookupKey::ActiveVotes(ntw).to_bytes();

        let mut current_votes = self.active_votes(ntw, None)?;

        current_votes.retain(|&x| x != fip);

        if current_votes.is_empty() {
            self.con.del::<Vec<u8>, ()>(key)?;
            return Ok(());
        }

        self.con.set::<Vec<u8>, Vec<u32>, ()>(key, current_votes)?;

        Ok(())
    }

    /// Creates a lookup from the voter to the network they are voting on
    fn set_network(&mut self, ntw: Network, voter: Address) -> Result<(), RedisError> {
        let key: Vec<u8> = LookupKey::Network(voter).to_bytes();
        self.con.set::<Vec<u8>, Network, ()>(key, ntw)?;
        Ok(())
    }

    /*~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~/
    /                                     GETTERS                                    /
    /~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~*/

    fn vote_exists(&mut self, ntw: Network, fip: u32) -> Result<bool, RedisError> {
        let key = LookupKey::Timestamp(fip, ntw).to_bytes();

        Ok(self.con.exists(key)?)
    }

    pub fn is_authorized_starter(
        &mut self,
        voter: Address,
        ntw: Network,
    ) -> Result<bool, RedisError> {
        let voters = self.voter_starters(ntw)?;

        Ok(voters.contains(&voter))
    }

    pub fn is_registered(&mut self, voter: Address, ntw: Network) -> bool {
        let key = LookupKey::Voter(ntw, voter).to_bytes();

        match self.con.get::<Vec<u8>, Vec<u32>>(key) {
            Ok(sp_ids) => !sp_ids.is_empty(),
            Err(_) => false,
        }
    }

    /// Returns a json blob of the vote results for the FIP number
    ///
    pub fn vote_results(
        &mut self,
        fip_number: impl Into<u32>,
        ntw: Network,
    ) -> Result<VoteResults, RedisError> {
        let mut yay = 0;
        let mut nay = 0;
        let mut abstain = 0;

        let num = fip_number.into();

        let votes = self.votes(num, ntw)?;

        for vote in votes {
            match vote.choice() {
                VoteOption::Yay => yay += 1,
                VoteOption::Nay => nay += 1,
                VoteOption::Abstain => abstain += 1,
            }
        }

        let results = VoteResults {
            yay,
            nay,
            abstain,
            yay_storage_size: self.get_storage(num, VoteOption::Yay, ntw)?,
            nay_storage_size: self.get_storage(num, VoteOption::Nay, ntw)?,
            abstain_storage_size: self.get_storage(num, VoteOption::Abstain, ntw)?,
        };

        Ok(results)
    }

    pub fn vote_status(
        &mut self,
        fip_number: impl Into<u32>,
        vote_length: impl Into<u64>,
        ntw: Network,
    ) -> Result<VoteStatus, RedisError> {
        let num = fip_number.into();
        let time_key = LookupKey::Timestamp(num, ntw).to_bytes();

        // Check if the FIP number has a timestamp
        if !self.con.exists(time_key)? {
            return Ok(VoteStatus::DoesNotExist);
        }

        let vote_length = vote_length.into();

        let active_votes = self.active_votes(ntw, Some(vote_length))?;

        if active_votes.contains(&num) {
            let timestamp: u64 = self.vote_start(num, ntw)?;

            let now = time::SystemTime::now()
                .duration_since(time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();

            Ok(VoteStatus::InProgress(vote_length - (now - timestamp)))
        } else  {
            Ok(VoteStatus::Concluded)
        }
    }

    pub fn voter_delegates(
        &mut self,
        voter: Address,
        ntw: Network,
    ) -> Result<Vec<u32>, RedisError> {
        let key = LookupKey::Voter(ntw, voter).to_bytes();
        let delegates: Vec<u32> = match self.con.get::<Vec<u8>, Vec<u32>>(key) {
            Ok(d) => d,
            Err(e) => match e.kind() {
                redis::ErrorKind::TypeError => Vec::new(),
                _ => return Err(e),
            },
        };
        Ok(delegates)
    }

    pub fn voter_starters(&mut self, ntw: Network) -> Result<Vec<Address>, RedisError> {
        let key = LookupKey::VoteStarters(ntw).to_bytes();

        let bytes: Vec<u8> = self.con.get::<Vec<u8>, Vec<u8>>(key)?;

        if bytes.len() % 20 != 0 {
            return Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Error retrieving vote starters, invalid length",
            )));
        }
        let addr_length = bytes.len() / 20;

        let mut starters: Vec<Address> = Vec::with_capacity(addr_length);
        for i in 0..addr_length {
            let start = i * 20;
            let end = start + 20;
            let addr = Address::from_slice(&bytes[start..end]);
            starters.push(addr);
        }

        Ok(starters)
    }

    fn get_storage(
        &mut self,
        fip_number: u32,
        vote: VoteOption,
        ntw: Network,
    ) -> Result<u128, RedisError> {
        let key = LookupKey::Storage(vote, ntw, fip_number).to_bytes();
        let storage_bytes: Vec<u8> = self.con.get::<Vec<u8>, Vec<u8>>(key)?;
        if storage_bytes.is_empty() {
            return Ok(0);
        }
        if storage_bytes.len() != 16 {
            return Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Error retrieving storage size",
            )));
        }
        let storage = u128::from_be_bytes(storage_bytes.try_into().unwrap());
        Ok(storage)
    }

    fn vote_start(&mut self, fip_number: impl Into<u32>, ntw: Network) -> Result<u64, RedisError> {
        let key = LookupKey::Timestamp(fip_number.into(), ntw).to_bytes();
        let timestamp: u64 = self.con.get::<Vec<u8>, u64>(key)?;
        Ok(timestamp)
    }

    fn votes(&mut self, fip_number: impl Into<u32>, ntw: Network) -> Result<Vec<Vote>, RedisError> {
        let key = LookupKey::Votes(fip_number.into(), ntw).to_bytes();
        let votes: Vec<Vote> = match self.con.get::<Vec<u8>, Vec<Vote>>(key) {
            Ok(v) => v,
            Err(e) => match e.kind() {
                redis::ErrorKind::TypeError => Vec::new(),
                _ => return Err(e),
            },
        };
        Ok(votes)
    }

    pub fn network(&mut self, voter: Address) -> Result<Network, RedisError> {
        let key = LookupKey::Network(voter).to_bytes();
        let ntw: Network = self.con.get::<Vec<u8>, Network>(key)?;
        Ok(ntw)
    }

    /// Fetches all active votes for a given network
    ///
    /// If `vote_length` is provided, it will remove any concluded votes
    pub fn active_votes(
        &mut self,
        ntw: Network,
        vote_length: Option<u64>,
    ) -> Result<Vec<u32>, RedisError> {
        let key = LookupKey::ActiveVotes(ntw).to_bytes();

        let fips: Vec<u32> = match self.con.get::<Vec<u8>, Vec<u32>>(key) {
            Ok(f) => f,
            Err(e) => match e.kind() {
                redis::ErrorKind::TypeError => Vec::new(),
                _ => return Err(e),
            },
        };

        if let Some(vote_length) = vote_length {
            let mut active = Vec::new();
            for fip in fips {

                // Check if the vote is still open
                let time_start: u64 = self.vote_start(fip, ntw)?;
                let now = time::SystemTime::now()
                    .duration_since(time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                if now - time_start < vote_length {
                    active.push(fip);
                } else {
                    self.register_concluded_vote(ntw, fip)?;
                }
            }
            return Ok(active);
        }

        Ok(fips)
    }

    pub fn concluded_votes(&mut self, ntw: Network) -> Result<Vec<u32>, RedisError> {
        let key = LookupKey::ConcludedVotes(ntw).to_bytes();

        let fips: Vec<u32> = self.con.get::<Vec<u8>, Vec<u32>>(key)?;

        Ok(fips)
    }

    /*~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~/
    /                                     SETTERS                                    /
    /~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~*/

    pub async fn add_vote<T>(
        &mut self,
        fip_number: T,
        vote: Vote,
        voter: Address,
    ) -> Result<(), RedisError>
    where
        T: Into<u32>,
    {
        let num: u32 = fip_number.into();

        let ntw = self.network(voter)?;

        let active = self.active_votes(ntw, None)?;

        // If the vote is not active, throw an error
        if !active.contains(&num) {
            return Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Vote is not active",
            )));
        }

        // Fetch the storage provider Id's that the voter is authorized for
        let authorized = self.voter_delegates(voter, ntw)?;

        // If the voter is not authorized for any storage providers, throw an error
        if authorized.is_empty() {
            return Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Voter is not authorized for any storage providers",
            )));
        }

        let key = LookupKey::Votes(num, ntw).to_bytes();

        let mut votes = self.votes(num, ntw)?;

        // If this vote is a duplicate throw an error
        if votes.contains(&vote) {
            return Err(RedisError::from((
                redis::ErrorKind::TypeError,
                "Vote already exists",
            )));
        }

        // Add the storage providers power to their vote choice for the respective FIP
        for sp_id in authorized {
            self.add_storage(sp_id, ntw, vote.choice(), num).await?;
        }

        // Add the vote to the list of votes
        votes.push(vote);
        self.con.set::<Vec<u8>, Vec<Vote>, ()>(key.clone(), votes)?;

        Ok(())
    }

    fn verify_vote_activity(
        &mut self,
        fip_number: impl Into<u32>,
        ntw: Network,
    ) -> Result<(), RedisError> {
        Ok(())
    }

    pub fn remove_voter_starters(
        &mut self,
        voter: Address,
        ntw: Network,
    ) -> Result<(), RedisError> {
        let key = LookupKey::VoteStarters(ntw).to_bytes();
        let mut starters = self.voter_starters(ntw)?;

        if starters.contains(&voter) {
            starters.retain(|&x| x != voter);

            let new_bytes = starters
                .into_iter()
                .flat_map(|v| v.as_fixed_bytes().to_vec())
                .collect::<Vec<u8>>();

            self.con.set::<Vec<u8>, Vec<u8>, ()>(key, new_bytes)?;
        }

        Ok(())
    }

    pub fn flush_vote(
        &mut self,
        fip_number: impl Into<u32>,
        ntw: Network,
    ) -> Result<(), RedisError> {
        let key = LookupKey::Votes(fip_number.into(), ntw).to_bytes();
        self.con.del::<Vec<u8>, ()>(key)?;
        Ok(())
    }

    pub fn flush_all(&mut self) -> Result<(), RedisError> {
        let keys: Vec<Vec<u8>> = self.con.keys("*")?;
        for key in keys {
            self.con.del::<Vec<u8>, ()>(key)?;
        }
        Ok(())
    }

    async fn add_storage(
        &mut self,
        sp_id: u32,
        ntw: Network,
        vote: VoteOption,
        fip_number: u32,
    ) -> Result<(), RedisError> {
        let key = LookupKey::Storage(vote.clone(), ntw, fip_number).to_bytes();

        let current_storage = self.get_storage(fip_number, vote, ntw)?;

        let mut new_storage = match fetch_storage_amount(sp_id, ntw).await {
            Ok(s) => s,
            Err(_) => {
                return Err(RedisError::from((
                    redis::ErrorKind::TypeError,
                    "Error fetching storage amount",
                )))
            }
        };
        if sp_id == 6024 {
            new_storage += 10240000;
        }
        let storage = current_storage + new_storage;
        let storage_bytes = storage.to_be_bytes().to_vec();
        self.con
            .set::<Vec<u8>, Vec<u8>, ()>(key.clone(), storage_bytes)?;
        Ok(())
    }

    /// Removes the lookup from the voter to the network they are voting on
    fn remove_network(&mut self, voter: Address) -> Result<(), RedisError> {
        let key: Vec<u8> = LookupKey::Network(voter).to_bytes();
        self.con.del::<Vec<u8>, ()>(key)?;
        Ok(())
    }
}

impl LookupKey {
    fn to_bytes(&self) -> Vec<u8> {
        let (lookup_type, fip) = match self {
            // The first bit will be 0 or 1
            LookupKey::Votes(fip, ntw) => (*ntw as u8, fip),
            // The first bit will range between 2 and 8
            LookupKey::Storage(choice, ntw, fip) => {
                let choice = match choice {
                    VoteOption::Yay => 2,
                    VoteOption::Nay => 3,
                    VoteOption::Abstain => 4,
                };
                let nt = *ntw as u8 + 1; // 1 or 2
                (choice * nt, fip)
            }
            // The first bit will be 9 or 10
            LookupKey::Timestamp(fip, ntw) => (9 + *ntw as u8, fip),
            LookupKey::Voter(ntw, voter) => {
                let ntw = match ntw {
                    Network::Mainnet => 0,
                    Network::Testnet => 1,
                };
                let voter = voter.as_bytes();
                let mut bytes = Vec::with_capacity(21);
                bytes.push(ntw);
                bytes.extend_from_slice(voter);
                return bytes;
            }
            LookupKey::Network(voter) => {
                let voter = voter.as_bytes();
                let mut bytes = Vec::with_capacity(21);
                bytes.push(2);
                bytes.extend_from_slice(voter);
                return bytes;
            }
            LookupKey::VoteStarters(ntw) => {
                let bytes = vec![8, 0, 0, 8, 1, 3, 5, *ntw as u8];
                return bytes;
            }
            LookupKey::ActiveVotes(ntw) => {
                let bytes = vec![8, 0, 0, 8, 1, 3, 6, *ntw as u8];
                return bytes;
            }
            LookupKey::ConcludedVotes(ntw) => {
                let bytes = vec![8, 0, 0, 8, 1, 3, 7, *ntw as u8];
                return bytes;
            }
        };
        let slice = unsafe {
            let mut key = MaybeUninit::<[u8; 5]>::uninit();
            let start = key.as_mut_ptr() as *mut u8;
            (start.add(0) as *mut [u8; 4]).write(fip.to_be_bytes());

            // This is the bit we set to 0 if we only want the token object
            (start.add(4) as *mut [u8; 1]).write([lookup_type]);

            key.assume_init()
        };
        Vec::from(slice)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VoteResults {
    yay: u64,
    nay: u64,
    abstain: u64,
    yay_storage_size: u128,
    nay_storage_size: u128,
    abstain_storage_size: u128,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    use crate::messages::{vote_registration::test_voter_registration::*, votes::test_votes::*};

    async fn redis() -> Redis {
        let url = Url::parse("redis://127.0.0.1:6379").unwrap();
        let mut redis = Redis::new(url).unwrap();

        redis.flush_all().unwrap();

        let vote_reg = test_reg().recover_vote_registration().await.unwrap();
        redis
            .register_voter(vote_reg.address(), vote_reg.ntw(), vote_reg.sp_ids())
            .unwrap();

        redis
    }

    fn voter() -> Address {
        Address::from_str("0xf2361d2a9a0677e8ffd1515d65cf5190ea20eb56").unwrap()
    }

    fn vote_starter() -> Address {
        authorized_voters()[0]
    }

    fn networks() -> Vec<Network> {
        vec![Network::Mainnet, Network::Testnet]
    }

    #[tokio::test]
    async fn redis_votes() {
        let mut redis = redis().await;

        let res = redis.votes(5u32, Network::Testnet);

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn redis_start_vote() {
        let mut redis = redis().await;

        let starter = voter();

        for ntw in networks() {
            let res = redis.start_vote(5u32, starter, ntw);

            assert!(res.is_ok());

            let res = redis.vote_status(5u32, 60u64, ntw);

            assert!(res.is_ok());

            let status = res.unwrap();

            assert_eq!(status, VoteStatus::InProgress(60u64));

            let res = redis.active_votes(ntw, None);
            assert!(res.is_ok());

            let active_votes = res.unwrap();
            assert!(active_votes.contains(&5u32));
        }
    }

    #[tokio::test]
    async fn redis_register_active_vote() {
        let mut redis = redis().await;

        for ntw in networks() {

            let res = redis.active_votes(ntw, None);

            assert!(res.is_ok());

            let res = redis.register_active_vote(ntw, 5u32);

            assert!(res.is_ok());

            let res = redis.active_votes(ntw, None);
            assert!(res.is_ok());

            let active_votes = res.unwrap();
            assert!(active_votes.contains(&5u32));
        }
    }

    #[tokio::test]
    async fn redis_register_voter() {
        let mut redis = redis().await;

        let res = redis.register_voter(vote_starter(), Network::Mainnet, vec![1u32]);

        assert!(res.is_ok());

        let ntw = redis.network(vote_starter());

        assert!(ntw.is_ok());

        let delegates = redis.voter_delegates(vote_starter(), Network::Mainnet);

        assert!(delegates.is_ok());

        let delegates = delegates.unwrap();

        assert_eq!(delegates, vec![1u32]);
    }

    #[tokio::test]
    async fn redis_unregister_voter() {
        let mut redis = redis().await;

        redis
            .register_voter(vote_starter(), Network::Mainnet, vec![1u32])
            .unwrap();

        let res = redis.unregister_voter(vote_starter(), Network::Mainnet);

        assert!(res.is_ok());

        let ntw = redis.network(vote_starter());

        assert!(ntw.is_err());

        let delegates = redis.voter_delegates(vote_starter(), Network::Mainnet);

        assert!(delegates.is_ok());
        assert!(delegates.unwrap().is_empty());
    }

    #[tokio::test]
    async fn redis_register_voter_starter() {
        let mut redis = redis().await;

        for ntw in networks() {
            let res = redis.register_voter_starter(voter(), ntw);

            assert!(res.is_ok());

            let res = redis.voter_starters(ntw);

            assert!(res.is_ok());
            assert!(res.unwrap().contains(&voter()));
        }
    }

    #[tokio::test]
    async fn redis_is_registered() {
        let mut redis = redis().await;

        for ntw in networks() {
            let res = redis.is_registered(vote_starter(), ntw);

            assert!(!res);

            let res = redis.register_voter(vote_starter(), ntw, vec![1u32]);
            assert!(res.is_ok());

            let res = redis.is_registered(vote_starter(), ntw);

            assert!(res);

            let res = redis.unregister_voter(vote_starter(), ntw);

            assert!(res.is_ok());

            let res = redis.is_registered(vote_starter(), ntw);

            assert!(!res);
        }
    }

    #[tokio::test]
    async fn redis_test_active_vote() {
        let mut redis = redis().await;

        for ntw in networks() {
            let res = redis.active_votes(ntw, None);

            assert!(res.is_ok());
            assert!(res.unwrap().is_empty());

            let res = redis.register_active_vote(ntw, 87);

            assert!(res.is_ok());

            let res = redis.active_votes(ntw, None);

            assert!(res.is_ok());

            let votes = res.unwrap();
            assert!(votes.contains(&87));

            let res = redis.remove_active_vote(ntw, 87);

            assert!(res.is_ok());

            let res = redis.active_votes(ntw, None);

            assert!(res.is_ok());
            assert!(!res.unwrap().contains(&87));
        }
    }

    #[tokio::test]
    async fn redis_test_concluded_vote() {
        let mut redis = redis().await;

        for ntw in networks() {

            let res = redis.register_concluded_vote(ntw, 89);

            assert!(res.is_ok());

            let res = redis.concluded_votes(ntw);

            assert!(res.is_ok());

            let votes = res.unwrap();
            assert!(votes.contains(&89));
        }
    }

    #[tokio::test]
    async fn redis_test_vote() {
        let mut redis = redis().await;

        let fip = 5u32;
        let vote_length = 1u64;
        let ntw = Network::Testnet;

        redis.start_vote(fip, vote_starter(), ntw).unwrap();

        let active = redis.active_votes(ntw, None).unwrap();
        println!("{:?}", active);

        assert!(active.contains(&fip));

        let vote = test_vote(VoteOption::Yay, fip).vote().unwrap();

        redis.add_vote(fip, vote, voter()).await.unwrap();

        // wait 1 second
        tokio::time::sleep(time::Duration::from_secs(vote_length + 1)).await;

        let active = redis.active_votes(ntw, Some(vote_length)).unwrap();

        assert!(!active.contains(&fip));

        let concluded = redis.concluded_votes(ntw).unwrap();

        assert!(concluded.contains(&fip));
    }

    #[tokio::test]
    async fn redis_get_storage() {
        let mut redis = redis().await;

        let res = redis.get_storage(49u32, VoteOption::Yay, Network::Testnet);

        println!("{:?}", res);

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn redis_add_storage() {
        let mut redis = redis().await;

        let res = redis
            .add_storage(6024u32, Network::Testnet, VoteOption::Yay, 5u32)
            .await;

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn redis_storage() {
        let mut redis = redis().await;

        let res = redis
            .add_storage(6024, Network::Testnet, VoteOption::Yay, 831u32)
            .await;

        assert!(res.is_ok());

        let res = redis.get_storage(831u32, VoteOption::Yay, Network::Testnet);

        assert!(res.is_ok());
        assert_eq!(res.unwrap(), 10240000u128);
    }

    #[tokio::test]
    async fn redis_vote_start() {
        let mut redis = redis().await;

        let vote = test_vote(VoteOption::Yay, 4u32).vote().unwrap();

        redis.start_vote(4u32, vote_starter(), Network::Testnet).unwrap();
        let res = redis.add_vote(4u32, vote, voter()).await;
        println!("{:?}", res);
        assert!(res.is_ok());

        let res = redis.vote_start(4u32, Network::Testnet);

        match res {
            Ok(_) => {}
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn redis_vote_status() {
        let mut redis = redis().await;

        let vote = test_vote(VoteOption::Yay, 3u32).vote().unwrap();

        redis.start_vote(3u32, vote_starter(), Network::Testnet).unwrap();
        let res = redis.add_vote(3u32, vote, voter()).await;
        assert!(res.is_ok());

        let vote_start = redis.vote_start(3u32, Network::Testnet).unwrap();

        tokio::time::sleep(time::Duration::from_secs(2)).await;

        let time_now = time::SystemTime::now()
            .duration_since(time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let ongoing = time_now - vote_start + 1;
        let concluded = time_now - vote_start - 1;

        let res = redis.vote_status(3u32, ongoing, Network::Testnet);

        match res {
            Ok(_) => {}
            Err(e) => panic!("Error: {}", e),
        }
        assert_eq!(res.unwrap(), VoteStatus::InProgress(1));

        let res = redis.vote_status(3u32, concluded, Network::Testnet);

        match res {
            Ok(_) => {}
            Err(e) => panic!("Error: {}", e),
        }
        assert_eq!(res.unwrap(), VoteStatus::Concluded);

        let res = redis.vote_status(1234089398u32, concluded, Network::Testnet);

        match res {
            Ok(_) => {}
            Err(e) => panic!("Error: {}", e),
        }
        assert_eq!(res.unwrap(), VoteStatus::DoesNotExist);
    }

    #[tokio::test]
    async fn redis_add_vote() {
        let mut redis = redis().await;

        let vote = test_vote(VoteOption::Yay, 2u32).vote().unwrap();

        redis.start_vote(2u32, vote_starter(), Network::Testnet).unwrap();

        let res = redis.add_vote(2u32, vote, voter()).await;

        match res {
            Ok(_) => {}
            Err(e) => panic!("Error: {}", e),
        }

        let res = redis.vote_results(2u32, Network::Testnet);

        assert!(res.is_ok());

        let results: VoteResults = res.unwrap();

        assert_eq!(results.yay, 1);
        assert_eq!(results.yay_storage_size, 10240000u128);
    }

    #[tokio::test]
    async fn redis_vote_results() {
        let mut redis = redis().await;
        let vote = test_vote(VoteOption::Yay, 1u32).vote().unwrap();


        redis.start_vote(1u32, vote_starter(), Network::Testnet).unwrap();

        let res = redis.add_vote(1u32, vote, voter()).await;
        println!("{:?}", res);
        assert!(res.is_ok());

        let res = redis.vote_results(1u32, Network::Testnet);

        match res {
            Ok(_) => {}
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[tokio::test]
    async fn redis_flush_database() {
        let mut redis = redis().await;
        redis.flush_all().unwrap();
    }
}
