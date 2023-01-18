// mod aggregator;
mod exchange;
mod exchanges;
mod grpc_server;

use std::{collections::HashMap, string::ToString};

use futures_util::{stream::SelectAll, StreamExt};
use tokio::sync::broadcast::channel as broadcast_channel;
use tokio_stream::wrappers::ReceiverStream;

use exchange::{Exchange, OrderBook};
use exchanges::{binance::Binance, bitstamp::Bitstamp};
use shared_types::proto::Summary;
use shared_types::TradedPair;

type BoxedOrderbook = Box<dyn OrderBook + Send>;

// change into an enum?
const ETH: &str = "ETH";
const BTC: &str = "BTC";

#[tokio::main]
async fn main() {
    println!("Starting order_book_service...\n");

    // todo this could be passed in via config or CLI arg.
    let eth_btc_pair = TradedPair::new(ETH.to_string(), BTC.to_string());

    // Set up exchange instances
    let binance = Binance::new();
    let bitstamp = Bitstamp::new();

    let exchanges: Vec<Box<dyn Exchange>> = vec![Box::new(binance), Box::new(bitstamp)];

    // Loop through each exchange in the above Vec. For each try to connect and get a stream for the desired traded-pair.
    // If the attempt fails retry for a number of times.
    // If successful push the receiver and break out of the retry loop.
    let mut orderbook_stream = SelectAll::new();
    for exchange in exchanges {
        let mut attempts = 0;
        let max_attempts = 5;

        while attempts < max_attempts {
            attempts += 1;
            match exchange.stream_order_book_for_pair(&eth_btc_pair) {
                Ok(rx) => {
                    orderbook_stream.push(ReceiverStream::new(rx));
                    break;
                }
                Err(err) => {
                    println!("{}", err);
                    println!(
                        "Unable to connect to {} for pair {}. Retrying...({}/{})",
                        exchange.name(),
                        eth_btc_pair,
                        attempts,
                        max_attempts
                    )
                }
            }
        }
    }

    // NOTE: is connection to a single exchange acceptable?
    if orderbook_stream.is_empty() {
        println!("Unable to connect to any exchanges, exiting...");
        return;
    }

    // The receiver is given to the gRPC server and is subscribed to for each new client.
    let (grpc_server_tx, grpc_server_rx) = broadcast_channel(100);

    let summary_generator_task = tokio::spawn(async move {
        let mut orderbook_buffer = HashMap::new();

        while let Some(orderbook) = orderbook_stream.next().await {
            orderbook_buffer.insert(orderbook.source(), orderbook);

            // If the buffer has more than one orderbook stored then we can generate a summary - this also clears the map to prevent stale data carrying over.
            // todo check timestamps are "close enough" - could be another config value for tolerance
            if orderbook_buffer.keys().len() > 1 {
                let summary =
                    merge_orderbooks_into_summary(orderbook_buffer.drain().map(|(_, value)| value));

                println!("Summary: {}", summary);

                let _ = grpc_server_tx.send(summary);
            }
        }
    });

    let grpc_server_handle = tokio::spawn(grpc_server::run_server(grpc_server_rx));

    let _ = tokio::join!(summary_generator_task, grpc_server_handle);
}

/// Construct a [Summary] from a collection of [OrderBook]s
pub(crate) fn merge_orderbooks_into_summary(
    orderbooks: impl Iterator<Item = BoxedOrderbook>,
) -> Summary {
    let mut asks = Vec::new();
    let mut bids = Vec::new();

    // Loop through order books extending the above vecs with best asks and bids from each.
    for ob in orderbooks {
        asks.append(&mut ob.best_asks(10));
        bids.append(&mut ob.best_bids(10));
    }

    // Sort the combined ask and bid vecs
    asks.sort_by(|a, b| a.partial_cmp(b).unwrap());
    bids.sort_by(|a, b| b.partial_cmp(a).unwrap());

    Summary {
        spread: asks[1].price - bids[1].price,
        asks: asks[..10].to_vec(),
        bids: bids[..10].to_vec(),
    }
}
