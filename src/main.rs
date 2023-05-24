use actix_web::{
    get, post, web, App, HttpResponse, HttpServer, Responder,
};

use fip_voting::{votes::RecievedVote, redis::Redis};

#[get("/filecoin/vote/{fip_number}")]
async fn get_votes(fip_number: web::Path<u32>) -> impl Responder {
    // Get the votes from the redis database

    HttpResponse::Ok().finish()
}

#[post("/filecoin/vote/{fip_number}")]
async fn register_vote<'a>(body: web::Bytes, fip_number: web::Path<u32>) -> impl Responder {
    // Deserialize the body into the vote struct
    let vote: RecievedVote = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::BadRequest().body(e.to_string());
        }
    };

    // Recover the vote
    let vote = match vote.recover_vote() {
        Ok(vote) => vote,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::BadRequest().body(e.to_string());
        }
    };

    // Open a connection to the redis database
    let mut redis = match Redis::new() {
        Ok(redis) => redis,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::InternalServerError().body(e.to_string());
        }
    };

    // Add the vote to the database
    match redis.add_vote(fip_number.into_inner(), vote) {
        Ok(_) => (),
        Err(e) => {
            println!("{}", e);
            return HttpResponse::InternalServerError().body(e.to_string());
        }
    }

    HttpResponse::Ok().finish()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(get_votes).service(register_vote))
        .bind("127.0.0.1:64459")?
        .run()
        .await
}
// 873126EDD5241C3B342B99B47DE787D8DC21AE3D003D2BB650FC0A6FCB42256021F8530396F4C4AFBF1390B1BDBD48355FD0FAF00C13145545AE52A0ACD1DE5C
