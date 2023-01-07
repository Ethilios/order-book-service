use std::fmt::{Display, Formatter};

use anyhow::Error;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver};
use tokio_tungstenite::connect_async;
use url::Url;

use crate::exchange::{Exchange, Order, OrderBook, TradedPair};

/// Refers to how many orders should be returned in the data set.
#[allow(unused)]
pub(crate) enum Level {
    Five,
    Ten,
    Twenty,
}

impl Display for Level {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Level::Five => write!(f, "5"),
            Level::Ten => write!(f, "10"),
            Level::Twenty => write!(f, "20"),
        }
    }
}

/// Refers to how often the order book should be checked for updates.
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

pub(crate) struct Binance {
    root_ws_endpoint: Url,
    depth: Level,
    update_frequency: UpdateSpeed,
}

impl Binance {
    pub(crate) fn new() -> Self {
        Self {
            root_ws_endpoint: Url::parse("wss://stream.binance.com:9443/ws").unwrap(),
            depth: Level::Ten,
            update_frequency: UpdateSpeed::Fast,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct PartialBookDepthResponse {
    #[serde(rename = "lastUpdateId")]
    #[allow(unused)]
    last_update_id: u64,
    bids: Vec<Order>,
    asks: Vec<Order>,
}

impl OrderBook for PartialBookDepthResponse {
    fn spread(&self) -> f32 {
        self.best_asks(1)[0].price - self.best_bids(1)[0].price
    }

    fn best_asks(&self, depth: usize) -> Vec<Order> {
        let mut asks = self.asks.clone();

        asks.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let depth_slice = &asks[..depth];

        Vec::from(depth_slice)
    }

    fn best_bids(&self, depth: usize) -> Vec<Order> {
        let mut bids = self.bids.clone();

        bids.sort_by(|a, b| b.partial_cmp(a).unwrap());

        let depth_slice = &bids[..depth];

        Vec::from(depth_slice)
    }
}

impl Exchange<PartialBookDepthResponse> for Binance {
    fn stream_order_book_for_pair(
        &self,
        traded_pair: &TradedPair,
    ) -> Result<Receiver<PartialBookDepthResponse>, Error> {
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
                        match serde_json::from_str::<PartialBookDepthResponse>(&msg.to_string()) {
                            Ok(order_book) => {
                                let _ = order_book_tx.send(order_book).await;
                            }
                            Err(serde_err) => println!("\nSerde Error:\n{}", serde_err),
                        }
                    }
                }
                Err(ws_err) => println!("\nWebsocket Error:\n{:?}", ws_err),
            }
        });

        Ok(order_book_rx)
    }
}
