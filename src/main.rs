use actix_web::{web, App, HttpServer};
use actix_cors::Cors;

use fip_voting::{
    Args,
    get::{get_votes, get_delegates, get_voting_power, get_vote_starters},
    post::{register_vote, register_voter, unregister_voter, register_vote_starter},
};


#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Parse the command line arguments
    let args = Args::new();
    let serve_address = args.serve_address();

    println!("Serving at {}", serve_address);
    let port = match serve_address.port() {
        Some(port) => port,
        None => 80
    };

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
            .service(register_vote)
            .service(register_voter)
            .service(unregister_voter)
            .service(register_vote_starter)
    })
    .bind((serve_address.host().unwrap().to_string(), port))?
    .run()
    .await
}
