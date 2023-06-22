use actix_web::{web, App, HttpServer};
use actix_cors::Cors;

use fip_voting::{
    Args,
    get_votes,
    register_vote, get_delegates, register_voter, unregister_voter, get_voting_power
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
            .service(register_vote)
            .service(get_delegates)
            .service(register_voter)
            .service(unregister_voter)
            .service(get_voting_power)
    })
    .bind((serve_address.host().unwrap().to_string(), port))?
    .run()
    .await
}
