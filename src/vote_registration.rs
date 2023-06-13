use std::{num::ParseIntError, str::FromStr};

use bls_signatures::{PublicKey, Serialize, Signature};
use ethers::types::Address;
use regex::Regex;
use serde::Deserialize;
use thiserror::Error;

use crate::storage::{verify_id, Network, StorageFetchError};

#[derive(Debug, Error)]
pub enum VoteRegistrationError {
    #[error("Signature does not match message")]
    SignatureMismatch,
    #[error("Invalid message format")]
    InvalidMessageFormat,
    #[error("Not a storage provider")]
    NotStorageProvider,
    #[error(transparent)]
    StorageFetchError(#[from] StorageFetchError),
    #[error("Invalid worker address")]
    InvalidWorkerAddress,
    #[error(transparent)]
    InvalidBlsEncoding(#[from] bls_signatures::Error),
    #[error(transparent)]
    InvalidHexEncoding(#[from] hex::FromHexError),
    #[error("Invalid address")]
    InvalidAddress,
    #[error("Invalid storage provider id")]
    InvalidStorageProviderId(#[from] ParseIntError),
}

/// Raw json to authorize an ethereum address
/// to vote on behalf of supplied storage provider Id's
///
/// Message scheme is the authorized eth voters then
/// the list of storage provider id's delimited by spaces
///
/// 0xabcdef0123456789 f0xxxx f0xxxx
#[derive(Deserialize)]
pub struct ReceivedVoterRegistration {
    signature: String,
    worker_address: String,
    message: String,
}

/// This struct represents an authorized eth address to vote on behalf
/// of a list of controlled storage providers
#[derive(Debug)]
pub struct VoterRegistration {
    authorized_voter: Address,
    network: Network,
    sp_ids: Vec<u32>,
}

impl VoterRegistration {
    pub fn address(&self) -> Address {
        self.authorized_voter.clone()
    }
    pub fn ntw(&self) -> Network {
        self.network.clone()
    }
    pub fn sp_ids(&self) -> Vec<u32> {
        self.sp_ids.clone()
    }
}

impl ReceivedVoterRegistration {
    pub async fn recover_vote_registration(
        &self,
    ) -> Result<VoterRegistration, VoteRegistrationError> {
        let (pubkey, ntw) = self.pub_key()?;

        let msg_hex = hex::decode(&self.message)?;

        match pubkey.verify(self.sig()?, &msg_hex) {
            true => (),
            false => return Err(VoteRegistrationError::SignatureMismatch),
        }

        let original = msg_hex.to_ascii_lowercase().iter().map(|b| *b as char).collect::<String>();

        let (addr, sp_ids) = match original
            .split_whitespace()
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
            .split_first()
        {
            Some((address, sp_ids)) => (Address::from_str(address), sp_ids.to_vec()),
            None => return Err(VoteRegistrationError::InvalidMessageFormat),
        };

        let address = match addr {
            Ok(addr) => addr,
            Err(_) => return Err(VoteRegistrationError::InvalidAddress),
        };

        let mut new_ids: Vec<u32> = Vec::new();
        for sp_id in sp_ids.clone() {
            match verify_id(sp_id.clone(), self.worker_address.clone(), ntw).await? {
                true => (),
                false => return Err(VoteRegistrationError::NotStorageProvider),
            };
            let id = u32::from_str(&sp_id[1..])?;
            new_ids.push(id);
        }

        Ok(VoterRegistration {
            authorized_voter: address,
            network: ntw,
            sp_ids: new_ids,
        })
    }

    fn pub_key(&self) -> Result<(PublicKey, Network), VoteRegistrationError> {
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
                    }
                    None => return Err(VoteRegistrationError::InvalidWorkerAddress),
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
                        }
                        None => return Err(VoteRegistrationError::InvalidWorkerAddress),
                    }
                }
                false => return Err(VoteRegistrationError::InvalidWorkerAddress),
            },
        };

        Ok((PublicKey::from_bytes(bytes.as_slice())?, ntw))
    }

    fn sig(&self) -> Result<Signature, VoteRegistrationError> {
        let bytes = hex::decode(&self.signature[2..])?;

        Ok(Signature::from_bytes(bytes.as_slice())?)
    }
}

#[cfg(test)]
mod vote_registration_tests {
    use super::*;

    fn test_reg() -> ReceivedVoterRegistration {
        ReceivedVoterRegistration { 
            signature: "0299f5c42a957809d0bd80cb29986b811fbacd1ed84b5995f1d21c6a7063cada725fe0c643bbcdc4082b078d1420fc9e7d08f9c28c9dbf4597183dd92c2fa2ff7727eee2e6f84fb24134051005ea93b3bfe5e294d2e1413bf111440afdadfa0744".to_string(), 
            worker_address: "t3qejyqmrirddrsb2w2thbaco3q6emuljumlhuonp3al35g3kkzx4zpeecycw7gim2meegemwot3gp3qr6alpa".to_string(), 
            message: "2030784632333631443241394130363737653866664431353135643635434635313930654132306542353620743036303234".to_string() 
        }
    }

    #[test]
    fn vote_registration_sig() {
        let reg = test_reg();
        let sig = reg.sig();

        assert!(sig.is_ok());
    }

    #[test]
    fn vote_registration_pub_key() {
        let reg = test_reg();
        let pub_key = reg.pub_key();

        assert!(pub_key.is_ok());

        let (_, ntw) = pub_key.unwrap();

        assert_eq!(ntw, Network::Testnet);
    }

    #[tokio::test]
    async fn vote_registration_recover() {
        let reg = test_reg();
        
        let res = reg.recover_vote_registration().await;

        println!("{:?}", res);
        assert!(res.is_ok());
    }
}
