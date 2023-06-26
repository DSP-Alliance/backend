use std::str::FromStr;

use ethers::types::{Address, Signature};
use serde::Deserialize;

use super::votes::VoteError;

#[derive(Deserialize, Debug)]
pub struct VoteStart {
    signature: String,
    pub message: String,
}

impl VoteStart {
    /// Returns a tuple of (signer, fip)
    pub fn auth(&self) -> Result<(Address, u32), VoteError> {
        let signer = self.pub_key()?;
        let fip = self.fip()?;

        Ok((signer, fip))
    }
    fn fip(&self) -> Result<u32, VoteError> {
        // Message is in the format "FIP-XXX"
        let fip = match self.message.split('-').nth(1) {
            Some(fip) => fip,
            None => return Err(VoteError::InvalidMessageFormat),
        };
        // convert to u32
        let fip = match fip.parse::<u32>() {
            Ok(fip) => fip,
            Err(_) => return Err(VoteError::InvalidMessageFormat),
        };
        Ok(fip)
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
