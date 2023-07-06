use std::{fs::File, io::BufReader};

use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};

use fip_voting::{
    authorized_voters,
    get::{
        get_active_votes, get_all_concluded_votes, get_concluded_votes, get_delegates,
        get_vote_starters, get_votes, get_voting_power,
    },
    post::{register_vote, register_vote_starter, register_voter, start_vote, unregister_voter},
    redis::Redis,
    storage::Network,
    Args,
};

fn load_certs() -> ServerConfig {
    let cert_file =
        &mut BufReader::new(File::open("/etc/letsencrypt/live/sp-vote.com/fullchain.pem").unwrap());
    let key_file =
        &mut BufReader::new(File::open("/etc/letsencrypt/live/sp-vote.com/privkey.pem").unwrap());

    let cert_chain = certs(cert_file)
        .unwrap()
        .into_iter()
        .map(rustls::Certificate)
        .collect::<Vec<_>>();
    let mut keys = pkcs8_private_keys(key_file)
        .unwrap()
        .into_iter()
        .map(rustls::PrivateKey)
        .collect::<Vec<_>>();

    if keys.is_empty() {
        panic!("No private keys found");
    }

    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth();
    config.with_single_cert(cert_chain, keys.remove(0)).unwrap()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Parse the command line arguments
    let args = Args::new();
    let serve_address = args.serve_address();

    let port = match serve_address.scheme() {
        "http" => 80,
        "https" => 443,
        _ => panic!("Invalid scheme"),
    };

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

    let server = HttpServer::new(move || {
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
            .service(get_all_concluded_votes)
            .service(register_vote)
            .service(register_voter)
            .service(unregister_voter)
            .service(register_vote_starter)
            .service(start_vote)
    });
    /*
    .bind((serve_address.host().unwrap().to_string(), port))?
    .run()
    .await*/

    if port == 443 {
        let certs = load_certs();

        println!("Serving over HTTPS at {}", serve_address);
        server.bind_rustls((serve_address.host().unwrap().to_string(), port), certs)?
    } else {
        println!("Serving over HTTP at {}", serve_address);
        server.bind((serve_address.host().unwrap().to_string(), port))?
    }
    .run()
    .await
}
