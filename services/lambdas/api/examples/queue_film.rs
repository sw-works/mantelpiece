//! Queue one day film on the deployed film Lambda, as `POST /trips/{trip}/film` does, without the access or day checks.
//!
//!     cargo run -p mantelpiece-api --example queue_film -- <film-function> <household> <trip> <day>
//!
//! Needs AWS credentials for the stack's account; prints the queue result.
use mantelpiece_api::films::{Films, LambdaFilms};
use mantelpiece_core::ids::{HouseholdId, TripId};

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let aws = aws_config::load_from_env().await;
    let films = LambdaFilms::from_config(&aws, a[1].clone());
    let r = films
        .queue(
            &HouseholdId::new(a[2].as_str()),
            &TripId::new(a[3].as_str()),
            a[4].parse().unwrap(),
        )
        .await;
    println!("{r:?}");
}
