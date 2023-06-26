use actix_cors::Cors;
use actix_web::{web, App, HttpServer};

use fip_voting::{
    authorized_voters,
    get::{
        get_active_votes, get_concluded_votes, get_delegates, get_vote_starters, get_votes,
        get_voting_power, get_storage, get_all_concluded_votes,
    },
    post::{register_vote, register_vote_starter, register_voter, start_vote, unregister_voter},
    redis::Redis,
    storage::Network,
    Args,
};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Parse the command line arguments
    let args = Args::new();
    let serve_address = args.serve_address();

    println!("Serving at {}", serve_address);
    let port = serve_address.port().unwrap_or(80);

    let mut redis = Redis::new(args.redis_path()).unwrap();

    let ntws = vec![Network::Mainnet, Network::Testnet];
    for ntw in ntws {
        let voter_starters = redis.voter_starters(ntw).unwrap();
        for voter in authorized_voters() {
            if voter_starters.contains(&voter) {
                continue;
            } else {
                redis.register_voter_starter(voter, ntw).unwrap();
            }
        }
    }

    HttpServer::new(move || {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(args.clone()))
            .service(get_votes)
            .service(get_voting_power)
            .service(get_vote_starters)
            .service(get_delegates)
            .service(get_concluded_votes)
            .service(get_active_votes)
            .service(get_storage)
            .service(get_all_concluded_votes)
            .service(register_vote)
            .service(register_voter)
            .service(unregister_voter)
            .service(register_vote_starter)
            .service(start_vote)
    })
    .bind((serve_address.host().unwrap().to_string(), port))?
    .run()
    .await
}
