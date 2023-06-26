use std::{str::FromStr, collections::HashMap};

use actix_web::{get, web, HttpResponse, Responder};
use ethers::types::Address;

use crate::{
    errors::*,
    redis::{Redis, VoteStatus},
    storage::{fetch_storage_amount, Network},
    Args, NtwAddrParams, NtwFipParams, NtwParams,
};

#[get("/filecoin/vote")]
async fn get_votes(
    query_params: web::Query<NtwFipParams>,
    config: web::Data<Args>,
) -> impl Responder {
    println!("votes requested");

    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => return HttpResponse::BadRequest().body(INVALID_NETWORK),
    };
    let num = query_params.fip_number;

    // Open a connection to the redis database
    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            let res = format!("{}: {}", OPEN_CONNECTION_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    // Get the status of the vote from the database
    let status = match redis.vote_status(num, config.vote_length(), ntw) {
        Ok(status) => status,
        Err(e) => {
            let res = format!("{}: {}", VOTE_STATUS_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    println!("Vote status: {:?} for FIP: {}", status, num);

    // Return the appropriate response
    match status {
        VoteStatus::InProgress(time_left) => HttpResponse::Ok().body(time_left.to_string()),
        VoteStatus::Concluded => {
            let vote_results = match redis.vote_results(num, ntw) {
                Ok(results) => results,
                Err(e) => {
                    let res = format!("{}: {}", VOTE_RESULTS_ERROR, e);
                    println!("{}", res);
                    return HttpResponse::InternalServerError().body(res);
                }
            };
            println!("Vote results: {:?}", vote_results);
            HttpResponse::Ok().json(vote_results)
        }
        VoteStatus::DoesNotExist => HttpResponse::NotFound().finish(),
    }
}

#[get("/filecoin/delegates")]
async fn get_delegates(
    query_params: web::Query<NtwAddrParams>,
    config: web::Data<Args>,
) -> impl Responder {
    println!("Delegates requested");

    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => return HttpResponse::BadRequest().body(INVALID_NETWORK),
    };
    let address = query_params.address.clone();

    let address = match Address::from_str(address.as_str()) {
        Ok(address) => address,
        Err(e) => {
            let res = format!("{}: {}", INVALID_ADDRESS, e);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    // Open a connection to the redis database
    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            let res = format!("{}: {}", OPEN_CONNECTION_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    // Get the status of the vote from the database
    let delegates = match redis.voter_delegates(address, ntw) {
        Ok(delegates) => delegates,
        Err(e) => {
            let res = format!("{}: {}", VOTER_DELEGATES_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    println!("Delegates: {:?} for address: {}", delegates, address);

    let mut dgts: Vec<String> = Vec::new();
    let prefix = match ntw {
        Network::Mainnet => "f",
        Network::Testnet => "t",
    };
    for delegate in delegates {
        dgts.push(format!("{}0{}", prefix, delegate));
    }

    HttpResponse::Ok().json(dgts)
}

#[get("/filecoin/votingpower")]
async fn get_voting_power(
    query_params: web::Query<NtwAddrParams>,
    config: web::Data<Args>,
) -> impl Responder {
    println!("Voting power requested");
    let address = query_params.address.clone();
    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => return HttpResponse::BadRequest().body(INVALID_NETWORK),
    };

    let address = match Address::from_str(address.as_str()) {
        Ok(address) => address,
        Err(e) => {
            let res = format!("{}: {}", INVALID_ADDRESS, e);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            let res = format!("{}: {}", OPEN_CONNECTION_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    let authorized = match redis.voter_delegates(address, ntw) {
        Ok(delegates) => delegates,
        Err(e) => {
            let res = format!("{}: {}", VOTER_DELEGATES_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    let mut voting_power = 0;
    for delegate in authorized {
        match fetch_storage_amount(delegate, ntw).await {
            Ok(amount) => voting_power += amount,
            Err(e) => {
                let res = format!("{}: {}", VOTING_POWER_ERROR, e);
                println!("{}", res);
                return HttpResponse::InternalServerError().body(res);
            }
        }
    }

    HttpResponse::Ok().body(voting_power.to_string())
}

#[get("/filecoin/voterstarters")]
async fn get_vote_starters(
    query_params: web::Query<NtwParams>,
    config: web::Data<Args>,
) -> impl Responder {
    println!("Vote starters requested");
    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => return HttpResponse::BadRequest().body(INVALID_NETWORK),
    };

    // Open a connection to the Redis Database
    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            let res = format!("{}: {}", OPEN_CONNECTION_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    // Get authorized vote starters
    let vote_starters = match redis.voter_starters(ntw) {
        Ok(vote_starters) => vote_starters,
        Err(e) => {
            let res = format!("{}: {}", VOTE_STARTERS_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    println!("Vote starters: {:?}", vote_starters);

    HttpResponse::Ok().json(vote_starters)
}

#[get("/filecoin/activevotes")]
async fn get_active_votes(
    query_params: web::Query<NtwParams>,
    config: web::Data<Args>,
) -> impl Responder {
    println!("Active votes requested");
    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => return HttpResponse::BadRequest().body(INVALID_NETWORK),
    };

    // Open a connection to the Redis Database
    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            let res = format!("{}: {}", OPEN_CONNECTION_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    // Get active votes
    let active_votes = match redis.active_votes(ntw, Some(config.vote_length())) {
        Ok(active_votes) => active_votes,
        Err(e) => {
            let res = format!("{}: {}", ACTIVE_VOTES_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    println!("Active votes: {:?}", active_votes);

    HttpResponse::Ok().json(active_votes)
}

#[get("/filecoin/votehistory")]
async fn get_concluded_votes(
    query_params: web::Query<NtwParams>,
    config: web::Data<Args>,
) -> impl Responder {
    println!("Concluded votes requested");
    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => return HttpResponse::BadRequest().body(INVALID_NETWORK),
    };

    // Open a connection to the Redis Database
    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            let res = format!("{}: {}", OPEN_CONNECTION_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    // Get concluded votes
    let concluded_votes = match redis.concluded_votes(ntw) {
        Ok(concluded_votes) => concluded_votes,
        Err(e) => {
            let res = format!("{}: {}", CONCLUDED_VOTES_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    println!("Concluded votes: {:?}", concluded_votes);

    HttpResponse::Ok().json(concluded_votes)
}

#[get("/filecoin/storage")]
async fn get_storage(
    query_params: web::Query<NtwAddrParams>,
    config: web::Data<Args>,
) -> impl Responder {
    println!("Storage requested");
    let address = query_params.address.clone();
    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => return HttpResponse::BadRequest().body(INVALID_NETWORK),
    };

    let address = match Address::from_str(address.as_str()) {
        Ok(address) => address,
        Err(e) => {
            let res = format!("{}: {}", INVALID_ADDRESS, e);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            let res = format!("{}: {}", OPEN_CONNECTION_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    let sp_ids = match redis.voter_delegates(address, ntw) {
        Ok(sp_ids) => sp_ids,
        Err(e) => {
            let res = format!("{}: {}", VOTER_DELEGATES_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    let mut storage = 0u128;
    for sp_id in sp_ids {
        let amount = match fetch_storage_amount(sp_id, ntw).await {
            Ok(amount) => amount,
            Err(e) => {
                let res = format!("{}: {}", STORAGE_ERROR, e);
                println!("{}", res);
                return HttpResponse::InternalServerError().body(res);
            }
        };
        storage += amount;
    }

    HttpResponse::Ok().body(storage.to_string())
}

#[get("/filecoin/allconcludedvotes")]
async fn get_all_concluded_votes(
    query_params: web::Query<NtwParams>,
    config: web::Data<Args>,
) -> impl Responder {
    println!("All concluded votes requested");

    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => return HttpResponse::BadRequest().body(INVALID_NETWORK),
    };

    // Open a connection to the Redis Database
    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            let res = format!("{}: {}", OPEN_CONNECTION_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    // Get concluded votes
    let concluded_votes = match redis.concluded_votes(ntw) {
        Ok(concluded_votes) => concluded_votes,
        Err(e) => {
            let res = format!("{}: {}", CONCLUDED_VOTES_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    let mut vote_res_map = HashMap::new();
    for vote in concluded_votes.into_iter() {
        let results = match redis.vote_results(vote, ntw) {
            Ok(results) => results,
            Err(e) => {
                let res = format!("{}: {}", VOTE_RESULTS_ERROR, e);
                println!("{}", res);
                return HttpResponse::InternalServerError().body(res);
            }
        };
        vote_res_map.insert(vote, results);
    }

    println!("Concluded votes: {:?}", vote_res_map);

    HttpResponse::Ok().json(vote_res_map)
}