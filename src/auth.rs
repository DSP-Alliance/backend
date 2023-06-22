use std::str::FromStr;

use ethers::{types::{Address, Signature}};
use serde::Deserialize;

use crate::votes::VoteError;


#[derive(Deserialize, Debug)]
pub struct VoterAuthorization {
    signature: String,
    message: String,
}

impl VoterAuthorization {
    /// Returns a tuple of (signer, authorized address)
    pub fn auth(&self) -> Result<(Address, Address), VoteError> {
        let signer = self.pub_key()?;
        let address = match Address::from_str(&self.message) {
            Ok(address) => address,
            Err(_) => return Err(VoteError::InvalidMessageFormat),
        };

        Ok((signer, address))
    }
    fn pub_key(&self) -> Result<Address, VoteError> {
        let signature = Signature::from_str(&self.signature)?;
        let msg = format!("\x19Ethereum Signed Message:\n{}{}", self.message.len(), self.message);
        let message_hash = ethers::utils::keccak256(msg);

        let address = signature.recover(message_hash)?;

        Ok(address)
    }
}