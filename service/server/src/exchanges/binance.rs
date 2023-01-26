use std::fmt::{Display, Formatter};

use anyhow::Error;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver};
use tokio::time::Instant;
use tokio_tungstenite::connect_async;
use url::Url;

use crate::exchange::{
    sort_orders_to_depth, BoxedExchange, BoxedOrderbook, Exchange, Order, OrderBook, Ordering,
};
use order_book_service_types::proto::{Level, TradedPair};

const BINANCE: &str = "Binance";
const BINANCE_WSS_URL: &str = "wss://stream.binance.com:9443/ws";

#[derive(Clone)]
pub(crate) struct Binance {
    root_ws_endpoint: Url,
    depth: Depth,
    update_frequency: UpdateSpeed,
}

impl Binance {
    pub(crate) fn new() -> Self {
        Self {
            root_ws_endpoint: Url::parse(BINANCE_WSS_URL).unwrap(),
            depth: Depth::Ten,
            update_frequency: UpdateSpeed::Fast,
        }
    }
}

impl Exchange for Binance {
    fn name(&self) -> &'static str {
        BINANCE
    }

    fn stream_order_book_for_pair(
        &self,
        traded_pair: &TradedPair,
    ) -> Result<Receiver<(BoxedOrderbook, Instant)>, Error> {
        let (order_book_tx, order_book_rx) = mpsc_channel(100);

        let order_book_url = Url::parse(
            format!(
                "{}/{}@depth{}@{}ms",
                self.root_ws_endpoint,
                traded_pair.symbol_lower(),
                self.depth,
                self.update_frequency
            )
            .as_str(),
        )?;

        tokio::spawn(async move {
            match connect_async(&order_book_url).await {
                Ok((mut ws_stream, _)) => {
                    while let Some(Ok(msg)) = ws_stream.next().await {
                        let received = Instant::now();
                        match serde_json::from_str::<PartialBookDepth>(&msg.to_string()) {
                            Ok(order_book) => {
                                let order_book: BoxedOrderbook = Box::new(order_book);
                                let _ = order_book_tx.send((order_book, received)).await;
                            }
                            Err(serde_err) => {
                                if msg.is_ping() {
                                    println!("Binance sent ping");
                                } else {
                                    println!("Serde Error: {serde_err}");
                                }
                            }
                        }
                    }
                }
                Err(ws_err) => println!("\nWebsocket Error:\n{ws_err}"),
            }
        });

        Ok(order_book_rx)
    }

    fn clone_dyn(&self) -> BoxedExchange {
        Box::new(self.clone())
    }
}

/// Refers to how many orders should be returned in the data set.
#[derive(Clone)]
#[allow(unused)]
pub(crate) enum Depth {
    Five,
    Ten,
    Twenty,
}

impl Display for Depth {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Depth::Five => write!(f, "5"),
            Depth::Ten => write!(f, "10"),
            Depth::Twenty => write!(f, "20"),
        }
    }
}

/// Refers to how often the order book should be checked for updates.
// #[allow(unused)]
#[derive(Clone)]
#[allow(unused)]
pub(crate) enum UpdateSpeed {
    /// Represents a frequency of 10 updates per second.
    Fast,
    /// Represents a frequency of 1 update per second
    Slow,
}

impl Display for UpdateSpeed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateSpeed::Fast => write!(f, "100"),
            UpdateSpeed::Slow => write!(f, "1000"),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct PartialBookDepth {
    #[serde(rename = "lastUpdateId")]
    #[allow(unused)]
    last_update_id: u64,
    bids: Vec<Order>,
    asks: Vec<Order>,
}

impl OrderBook for PartialBookDepth {
    fn source(&self) -> &'static str {
        BINANCE
    }

    fn spread(&self) -> f64 {
        self.best_asks(1)[0].price - self.best_bids(1)[0].price
    }

    fn best_asks(&self, depth: usize) -> Vec<Level> {
        sort_orders_to_depth(
            self.asks.clone(),
            Ordering::LowToHigh,
            depth,
            &self.source(),
        )
    }

    fn best_bids(&self, depth: usize) -> Vec<Level> {
        sort_orders_to_depth(
            self.bids.clone(),
            Ordering::HighToLow,
            depth,
            &self.source(),
        )
    }
}
