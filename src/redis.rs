extern crate redis;

use redis::{Commands, Connection, RedisError};

use crate::votes::Vote;

pub struct Redis {
    con: Connection,
}
/*
   TODO: Set up table for tracking storage size of votes
*/

impl Redis {
    pub fn new() -> Result<Redis, RedisError> {
        let client = redis::Client::open("redis://127.0.0.1:6379")?;
        let con = client.get_connection()?;

        Ok(Self { con })
    }

    // INITIALIZATION

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
        // Set a map of FIP number to vector of all votes
        self.con
            .set::<u32, Vec<Vote>, ()>(fip_number.into(), vote)?;

        // Set a map of FIP to timestamp of vote start
        
        Ok(())
    }

    // GETTERS

    pub fn votes(&mut self, fip_number: impl Into<u32>) -> Result<Vec<Vote>, RedisError> {
        let votes: Vec<Vote> = self.con.get::<u32, Vec<Vote>>(fip_number.into())?;
        Ok(votes)
    }

    // SETTERS

    pub fn add_vote<T>(&mut self, fip_number: T, vote: Vote) -> Result<(), RedisError>
    where
        T: Into<u32>,
    {
        let num: u32 = fip_number.into();

        if self.votes(num)?.is_empty() {
            self.new_vote(num, Some(vote))?;
            return Ok(());
        }

        let mut votes: Vec<Vote> = self.con.get::<u32, Vec<Vote>>(num)?;
        votes.push(vote);
        self.con.set::<u32, Vec<Vote>, ()>(num, votes)?;
        Ok(())
    }
}
