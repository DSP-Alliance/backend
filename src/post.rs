use actix_web::{post, web, HttpResponse, Responder};

use crate::{
    errors::*,
    messages::{
        auth::VoterAuthorization, vote_registration::ReceivedVoterRegistration, votes::ReceivedVote, vote_start::VoteStart,
    },
    redis::{Redis, VoteStatus},
    storage::Network,
    Args, FipParams, NtwParams,
};

#[post("/filecoin/vote")]
async fn register_vote(
    body: web::Bytes,
    query_params: web::Query<FipParams>,
    config: web::Data<Args>,
) -> impl Responder {
    let num = query_params.fip_number;

    println!("Vote received for FIP: {}", num);
    // Deserialize the body into the vote struct
    let vote: ReceivedVote = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            let res = format!("{}: {}", VOTE_DESERIALIZE_ERROR, e);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    // Recover the vote
    let vote = match vote.vote() {
        Ok(vote) => vote,
        Err(e) => {
            let res = format!("{}: {}", VOTE_RECOVER_ERROR, e);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    let voter = vote.voter();

    // Open a connection to the redis database
    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            let res = format!("{}: {}", OPEN_CONNECTION_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    let ntw = match redis.network(voter) {
        Ok(ntw) => ntw,
        Err(e) => {
            let res = format!("{}: {}", INVALID_ADDRESS, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    let status = match redis.vote_status(num, config.vote_length(), ntw) {
        Ok(status) => status,
        Err(e) => {
            let res = format!("{}: {}", VOTE_STATUS_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    };

    match status {
        VoteStatus::InProgress(_) => (),
        VoteStatus::Concluded => {
            let resp = format!("Vote concluded for FIP: {}", num);
            println!("{}", resp);
            return HttpResponse::Forbidden().body(resp);
        }
        VoteStatus::DoesNotExist => (),
    }

    let choice = vote.choice();

    // Add the vote to the database
    match redis.add_vote(num, vote, voter).await {
        Ok(_) => (),
        Err(e) => {
            let res = format!("{}: {}", VOTE_ADD_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    }

    println!("Vote ({:?}) added for FIP: {}", choice, num);

    HttpResponse::Ok().finish()
}

#[post("/filecoin/startvote")]
async fn start_vote(
    body: web::Bytes,
    query_params: web::Query<NtwParams>,
    config: web::Data<Args>
) -> impl Responder {
    println!("Vote start received");

    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => {
            let res = format!("{}: {}", INVALID_NETWORK, query_params.network);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    // Deserialize the body into the vote start struct
    let start: VoteStart = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            let res = format!("{}: {}", VOTE_DESERIALIZE_ERROR, e);
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

    let (starter, fip) = match start.auth() {
        Ok(auth) => auth,
        Err(e) => {
            let res = format!("{}: {}", VOTER_AUTH_ERROR, e);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    match redis.start_vote(fip, starter, ntw).await {
        Ok(_) => (),
        Err(e) => {
            let res = format!("{}: {}", VOTE_ADD_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    }

    HttpResponse::Ok().body(config.vote_length().to_string())
}

#[post("/filecoin/registerstarter")]
async fn register_vote_starter(
    query_params: web::Query<NtwParams>,
    body: web::Bytes,
    config: web::Data<Args>,
) -> impl Responder {
    let ntw = match query_params.network.as_str() {
        "mainnet" => Network::Mainnet,
        "calibration" => Network::Testnet,
        _ => return HttpResponse::BadRequest().body(INVALID_NETWORK),
    };

    let auth: VoterAuthorization = match serde_json::from_slice(&body) {
        Ok(auth) => auth,
        Err(e) => {
            let res = format!("{}: {}", VOTER_AUTH_DESERIALIZE_ERROR, e);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    let (signer, new_signer) = match auth.auth() {
        Ok(signer) => signer,
        Err(e) => {
            let res = format!("{}: {}", VOTER_AUTH_RECOVER_ERROR, e);
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

    match redis.is_authorized_starter(signer, ntw) {
        Ok(true) => (),
        Ok(false) => {
            let res = format!("{}: {}", VOTER_NOT_AUTHORIZED_ERROR, signer);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
        Err(e) => {
            let res = format!("{}: {}", VOTER_AUTH_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    }

    match redis.register_voter_starter(vec![new_signer], ntw) {
        Ok(_) => (),
        Err(e) => {
            let res = format!("{}: {}", VOTE_ADD_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    }

    HttpResponse::Ok().finish()
}

#[post("/filecoin/register")]
async fn register_voter(body: web::Bytes, config: web::Data<Args>) -> impl Responder {
    println!("Voter registration received");

    // Deserialize the body into the vote struct
    let reg: ReceivedVoterRegistration = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            let res = format!("{}: {}", VOTE_DESERIALIZE_ERROR, e);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    let registration = match reg.recover_vote_registration().await {
        Ok(registration) => registration,
        Err(e) => {
            let res = format!("{}: {}", VOTE_RECOVER_ERROR, e);
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

    // Add the vote to the database
    match redis.register_voter(registration) {
        Ok(_) => (),
        Err(e) => {
            let res = format!("{}: {}", VOTE_ADD_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    }

    HttpResponse::Ok().finish()
}

#[post("/filecoin/unregister")]
async fn unregister_voter(body: web::Bytes, config: web::Data<Args>) -> impl Responder {
    println!("Voter unregistration received");

    let reg: ReceivedVoterRegistration = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            let res = format!("{}: {}", VOTE_DESERIALIZE_ERROR, e);
            println!("{}", res);
            return HttpResponse::BadRequest().body(res);
        }
    };

    let registration = match reg.recover_vote_registration().await {
        Ok(registration) => registration,
        Err(e) => {
            let res = format!("{}: {}", VOTE_RECOVER_ERROR, e);
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

    match redis.unregister_voter(registration) {
        Ok(_) => (),
        Err(e) => {
            let res = format!("{}: {}", VOTE_ADD_ERROR, e);
            println!("{}", res);
            return HttpResponse::InternalServerError().body(res);
        }
    }

    HttpResponse::Ok().finish()
}

