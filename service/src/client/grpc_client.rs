pub mod orderbook {
    tonic::include_proto!("orderbook");
}

use anyhow::Error;
use orderbook::orderbook_aggregator_client::OrderbookAggregatorClient;
use orderbook::Empty;
use tonic::transport::Channel;
use tonic::{Request, Response, Status};

use shared_types::TradedPair;

#[tokio::main]
#[allow(unused)]
async fn main() {
    let mut client = OrderbookAggregatorClient::connect("http://0.0.0.0:3030")
        .await
        .unwrap();

    get_order_book_for_pair(
        &mut client,
        TradedPair::new("ETH".to_string(), "BTC".to_string()),
    )
    .await;
}

async fn get_order_book_for_pair(
    client: &mut OrderbookAggregatorClient<Channel>,
    _traded_pair: TradedPair,
) -> Result<(), Error> {
    let mut stream = client
        .book_summary(Request::new(Empty {}))
        .await?
        .into_inner();

    while let Some(summary) = stream.message().await? {
        println!("Output: {:?}", summary);
    }

    Ok(())
}
