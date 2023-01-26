mod aggregator;
mod exchange;
mod exchanges;
mod grpc_server;

use tokio::sync::mpsc::channel as mpsc_channel;

use crate::{
    aggregator::OrderbookAggregator,
    exchange::BoxedExchange,
    exchanges::{binance::Binance, bitstamp::Bitstamp},
    grpc_server::start_server,
};

#[tokio::main]
async fn main() {
    // Config can be brought in here

    run().await
}

async fn run() {
    println!("Starting orderbook service...");

    // Set up exchange instances
    let binance = Binance::new();
    let bitstamp = Bitstamp::new();

    let exchanges: Vec<BoxedExchange> = vec![Box::new(binance), Box::new(bitstamp)];

    // Creates a channel for the gRPC server to inform the process of new requests
    let (new_subscriber_tx, mut new_subscriber_rx) = mpsc_channel(100);

    // Spin up the gRPC server
    let grpc_server_handle = tokio::spawn(start_server(new_subscriber_tx, 3030));

    // Handle requests from the gRPC server
    let request_handler_handle = tokio::spawn(async move {
        // Await new subscription requests
        while let Some((requested_pair, summary_receiver_sender)) = new_subscriber_rx.recv().await {
            println!("MAIN :: New request for {requested_pair}");

            // There is no aggregator for the requested pair - a new one needs to be created.
            let new_aggregator = OrderbookAggregator::new(&exchanges, requested_pair.clone());

            // Send a receiver for the new aggregator back to the gRPC server to provide the orderbooks for the request.
            // This receiver will be cached in the gRPC server to minimise requests to the main process.
            let _ = summary_receiver_sender.send(new_aggregator.subscribe());

            // Start the aggregator
            tokio::spawn(new_aggregator.start());
        }
    });

    // todo this result should be handled to
    let _ = tokio::join!(grpc_server_handle, request_handler_handle);
}

#[cfg(test)]
mod tests {
    use futures_util::StreamExt;
    use order_book_service_client::{connect_to_summary_service, ConnectionSettings};
    use order_book_service_types::proto::TradedPair;
    use std::time::Duration;
    use url::Url;

    use crate::run;

    #[tokio::test]
    async fn should_provide_summaries_via_grpc() {
        tokio::spawn(run());

        let connection_settings = ConnectionSettings {
            server_address: Url::parse("http://0.0.0.0:3030").unwrap(),
            traded_pair: TradedPair::new("ETH".to_string(), "BTC".to_string()),
            max_attempts: 10,
            delay_between_attempts: Duration::from_secs(1),
        };

        let mut summary_receiver = connect_to_summary_service(connection_settings).await;
        let mut count = 0;

        while let Some(Ok(summary)) = summary_receiver.next().await {
            count += 1;
            println!("Summary: {summary}");
            if count >= 5 {
                break;
            }
        }
    }
}