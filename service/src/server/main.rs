// mod aggregator;
mod exchange;
mod exchanges;
mod grpc_server;

use std::collections::{HashMap, VecDeque};
use std::string::ToString;

use anyhow::Error;
use futures::stream::select_all;
use futures_util::stream::SelectAll;
use futures_util::StreamExt;
use tokio::{
    sync::{broadcast::channel as broadcast_channel, mpsc::Receiver},
    task::JoinSet,
};
use tokio_stream::wrappers::ReceiverStream;

use crate::exchange::{best_orders_to_depth, Ordering};
use crate::exchanges::binance::PartialBookDepthResponse;
use crate::exchanges::bitstamp::LiveOrderBookResponse;
use exchange::{Exchange, OrderBook};
use exchanges::{binance::Binance, bitstamp::Bitstamp};
use shared_types::proto::{Level, Summary};
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

    let mut orderbook_buffer = HashMap::new();

    while let Some(orderbook) = orderbook_stream.next().await {
        orderbook_buffer.insert(orderbook.source(), orderbook);

        // If the buffer has more than one orderbook stored then we can generate a summary
        // todo check timestamps are "close enough" - could be another config value for tolerance
        if orderbook_buffer.keys().len() > 1 {
            let summary =
                merge_orderbooks_into_summary(orderbook_buffer.drain().map(|(_, value)| value));

            println!("Summary: {}", summary);
        }

        // println!("Orderbook from: {}", orderbook.source())
    }

    // The receiver is given to the gRPC server and is subscribed to for each new client.
    let (grpc_server_tx, grpc_server_rx) = broadcast_channel(100);

    let orderbook_merger_handle = tokio::spawn(async move {
        // vec of options

        // tokio select on recvs *****

        // fill vector

        // check for two Somes

        // merge into orderbook then flush vector

        // repeat

        // loop {
        //     let mut task_set = JoinSet::new();
        //
        //     let bin_stream = ReceiverStream::new(binance_recv.recv());
        //     let bit_stream = ReceiverStream::new(bitstamp_recv.recv());
        //
        //     let zipped = bin_stream.zip(bit_stream);
        //
        //     while let x = zipped.await {}
        //     task_set.spawn(binance_recv.recv());
        //     task_set.spawn(bitstamp_recv.recv());
        //
        //     let mut orderbooks = Vec::new();
        //
        //     while let Some(res) = task_set.join_next().await {
        //         if let Ok(Some(orderbook)) = res {
        //             orderbooks.push(orderbook);
        //         }
        //
        //         // todo handle error case
        //     }
        //
        //     let summary = merge_orderbooks_into_summary(orderbooks.into_iter());
        //
        //     // Send the grpc server the summaries orderbook
        //     let _ = grpc_server_tx.send(summary);
        // }
    });

    let grpc_server_handle = tokio::spawn(grpc_server::run_server(grpc_server_rx));

    let _ = tokio::join!(orderbook_merger_handle, grpc_server_handle);
}

struct OrderbookBuffer {
    buffer: HashMap<String, BoxedOrderbook>,
}

impl OrderbookBuffer {
    fn new(num_sources: usize) -> Self {
        Self {
            buffer: HashMap::with_capacity(num_sources),
        }
    }

    fn push(&mut self, orderbook: BoxedOrderbook) {
        self.buffer.insert(orderbook.source(), orderbook);
    }

    fn check_for_comparable_books(&mut self) {}
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
        asks: asks[..5].to_vec(),
        bids: bids[..5].to_vec(),
    }
}

// 1. impl Type issue
// 2. impl Trait on external type - Create local type and then impl From to go between.
