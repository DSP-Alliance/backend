use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};

use fip_voting::{
    Args,
    redis::{Redis, VoteStatus},
    votes::ReceivedVote,
};

const OPEN_CONNECTION_ERROR: &str = "Error opening connection to in-memory database";
const VOTE_STATUS_ERROR: &str = "Error getting vote status";
const VOTE_RESULTS_ERROR: &str = "Error getting vote results";
const VOTE_DESERIALIZE_ERROR: &str = "Error deserializing vote";
const VOTE_RECOVER_ERROR: &str = "Error recovering vote";
const VOTE_ADD_ERROR: &str = "Error adding vote";

#[get("/filecoin/vote/{fip_number}")]
async fn get_votes(fip_number: web::Path<u32>, config: web::Data<Args>) -> impl Responder {
    println!("votes requested");

    let num = fip_number.into_inner();

    // Open a connection to the redis database
    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::InternalServerError().body(OPEN_CONNECTION_ERROR);
        }
    };

    // Get the status of the vote from the database
    let status = match redis.vote_status(num, config.vote_length()) {
        Ok(status) => status,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::InternalServerError().body(VOTE_STATUS_ERROR);
        }
    };

    println!("Vote status: {:?} for FIP: {}", status, num);

    // Return the appropriate response
    match status {
        VoteStatus::InProgress => HttpResponse::Forbidden().finish(),
        VoteStatus::Concluded => {
            let vote_results = match redis.vote_results(num) {
                Ok(results) => results,
                Err(e) => {
                    println!("{}", e);
                    return HttpResponse::InternalServerError().body(VOTE_RESULTS_ERROR);
                }
            };
            HttpResponse::Ok().json(vote_results)
        }
        VoteStatus::DoesNotExist => HttpResponse::NotFound().finish(),
    }
}

#[post("/filecoin/vote/{fip_number}")]
async fn register_vote(
    body: web::Bytes,
    fip_number: web::Path<u32>,
    config: web::Data<Args>,
) -> impl Responder {
    let num = fip_number.into_inner();

    println!("Vote received for FIP: {}", num);
    // Deserialize the body into the vote struct
    let vote: ReceivedVote = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::BadRequest().body(VOTE_DESERIALIZE_ERROR);
        }
    };

    let spid = vote.sp_id.clone();

    // Recover the vote
    let vote = match vote.recover_vote(num).await {
        Ok(vote) => vote,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::BadRequest().body(VOTE_RECOVER_ERROR);
        }
    };

    // Open a connection to the redis database
    let mut redis = match Redis::new(config.redis_path()) {
        Ok(redis) => redis,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::InternalServerError().body(OPEN_CONNECTION_ERROR);
        }
    };

    let status = match redis.vote_status(num, config.vote_length()) {
        Ok(status) => status,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::InternalServerError().body(VOTE_STATUS_ERROR);
        }
    };

    match status {
        VoteStatus::InProgress => (),
        VoteStatus::Concluded => {
            println!("Vote concluded for FIP: {}", num);
            return HttpResponse::Forbidden().finish();
        }
        VoteStatus::DoesNotExist => (),
    }

    let choice = vote.choice.clone();

    // Add the vote to the database
    match redis.add_vote(num, vote) {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e);
            return HttpResponse::InternalServerError().body(VOTE_ADD_ERROR);
        }
    }

    println!("Vote ({:?}) added for FIP: {} from Storage Provider: {}", choice, num, spid);

    HttpResponse::Ok().finish()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Parse the command line arguments
    let args = Args::new();
    let serve_address = args.serve_address();

    println!("Serving at {}", serve_address);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(args.clone()))
            .service(get_votes)
            .service(register_vote)
    })
    .bind((serve_address.host().unwrap().to_string(), serve_address.port().unwrap()))?
    .run()
    .await
}
