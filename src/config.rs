use std::env;

use url::Url;

pub struct Config {
    pub redis_path: Url,
    pub vote_length: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            redis_path: Url::parse("redis://127.0.0.1:6379").unwrap(),
            vote_length: 60 * 60 * 24 * 7,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let redis_path = env::var("REDIS_PATH").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let redis_path = Url::parse(&redis_path).unwrap();

        let vote_length = env::var("VOTE_LENGTH").unwrap_or_else(|_| "604800".to_string());
        let vote_length = vote_length.parse().unwrap();

        Self {
            redis_path,
            vote_length,
        }
    }
}