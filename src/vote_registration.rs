use std::{str::FromStr, num::ParseIntError};

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
    pub async fn recover_vote_registration(&self) -> Result<VoterRegistration, VoteRegistrationError> {
        let (pubkey, ntw) = self.pub_key()?;

        match pubkey.verify(self.sig()?, self.message.as_bytes()) {
            true => (),
            false => return Err(VoteRegistrationError::SignatureMismatch),
        }

        let (addr, sp_ids) = match self.message.split_whitespace().map(|s| s.to_string()).collect::<Vec<String>>().split_first() {
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
            let id = u32::from_str(sp_id.as_str())?;
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