use std::time::Duration;

use clap::Parser;
use tokio_stream::StreamExt;
use url::Url;

use order_book_service_client::{connect_to_summary_service, ConnectionSettings};
use order_book_service_types::proto::TradedPair;

/// Subscribe to the order book service for a traded pair
#[derive(Parser)]
struct Cli {
    /// Server address to bind
    address: String,
    /// The first symbol of the desired pair
    first: String,
    /// The second symbol of the desired pair
    second: String,
}

#[tokio::main]
async fn main() {
    println!("Orderbook Service CLI");

    let Cli {
        address,
        first,
        second,
    } = Cli::parse();

    let traded_pair = TradedPair { first, second };
    let server_address = Url::parse(&address).expect("Provided URL was not valid");

    let connection_settings = ConnectionSettings {
        server_address,
        traded_pair,
        max_attempts: 10,
        delay_between_attempts: Duration::from_millis(500),
    };

    let mut summary_stream = connect_to_summary_service(connection_settings).await;

    while let Some(Ok(summary)) = summary_stream.next().await {
        println!("{summary}");
    }
}
