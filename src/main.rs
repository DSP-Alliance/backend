use actix_web::{web, App, HttpServer};

use fip_voting::{
    Args,
    get_votes,
    register_vote,
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
        App::new()
            .app_data(web::Data::new(args.clone()))
            .service(get_votes)
            .service(register_vote)
    })
    .bind((serve_address.host().unwrap().to_string(), port))?
    .run()
    .await
}
