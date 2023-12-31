// Error messages
pub const OPEN_CONNECTION_ERROR: &str = "Error opening connection to in-memory database";

pub const VOTE_STATUS_ERROR: &str = "Error getting vote status";
pub const VOTE_RESULTS_ERROR: &str = "Error getting vote results";
pub const VOTE_DESERIALIZE_ERROR: &str = "Error deserializing vote";
pub const VOTE_RECOVER_ERROR: &str = "Error recovering vote";
pub const VOTE_ADD_ERROR: &str = "Error adding vote";

pub const VOTER_AUTH_DESERIALIZE_ERROR: &str = "Error deserializing voter authorization";
pub const VOTER_AUTH_RECOVER_ERROR: &str = "Error recovering voter authorization";
pub const VOTER_NOT_AUTHORIZED_ERROR: &str = "Voter not authorized to add new signers";
pub const VOTER_AUTH_ERROR: &str = "Error getting voter authorization";
pub const VOTER_DELEGATES_ERROR: &str = "Error getting voter delegates";

pub const VOTE_START_ERROR: &str = "Error starting vote";

pub const VOTE_EXISTS_ERROR: &str = "Error checking if vote exists";

pub const VOTE_STARTERS_ERROR: &str = "Error getting vote starters";

pub const VOTING_POWER_ERROR: &str = "Error getting voting power";

pub const STORAGE_ERROR: &str = "Error getting storage";

pub const SERDE_ERROR: &str = "Error serializing/deserializing";

pub const ACTIVE_VOTES_ERROR: &str = "Error getting active votes";
pub const VOTE_IS_ALREADY_STARTED: &str = "Vote is already started";
pub const VOTE_ALREADY_EXISTS: &str = "Vote already exists";
pub const CONCLUDED_VOTES_ERROR: &str = "Error getting concluded votes";

pub const VOTER_NOT_REGISTERED_NETWORK: &str = "Voter is not registered for this network";

pub const INVALID_NETWORK: &str = "Voter is not registered for this network";
pub const INVALID_ADDRESS: &str = "Invalid address";
