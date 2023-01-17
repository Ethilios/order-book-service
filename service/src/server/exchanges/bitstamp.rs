use anyhow::Error;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt::{Display, Formatter};
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

use crate::exchange::{best_orders_to_depth, Exchange, Order, OrderBook, Ordering};
use shared_types::{proto::Level, TradedPair};

const BITSTAMP: &str = "Bitstamp";
const BITSTAMP_WSS_URL: &str = "wss://ws.bitstamp.net";
const BTS_SUBSCRIBE: &str = "bts:subscribe";
const ORDERBOOK_CHANNEL: &str = "order_book_";

pub(crate) struct Bitstamp {
    root_ws_endpoint: Url,
}

impl Bitstamp {
    pub(crate) fn new() -> Self {
        Self {
            root_ws_endpoint: Url::parse(BITSTAMP_WSS_URL).unwrap(),
        }
    }
}

impl Exchange for Bitstamp {
    fn name(&self) -> String {
        BITSTAMP.to_string()
    }

    fn stream_order_book_for_pair(
        &self,
        traded_pair: &TradedPair,
    ) -> Result<Receiver<Box<dyn OrderBook + Send>>, Error> {
        let (order_book_tx, order_book_rx) = mpsc_channel(100);

        let ws_url = self.root_ws_endpoint.to_string();
        let symbol = traded_pair.symbol_lower();

        tokio::spawn(async move {
            match connect_async(ws_url).await {
                Ok((mut ws_stream, _)) => {
                    let channel = Channel::new(format!("{}{}", ORDERBOOK_CHANNEL, symbol));
                    let channel_sub_request = ChannelSubscriptionRequest::new(channel.clone());

                    ws_stream
                        .send(Message::Text(
                            serde_json::to_string(&channel_sub_request).unwrap(),
                        ))
                        .await
                        .unwrap();

                    // Handle initial response to subscription request
                    if let Some(subscription_response) = ws_stream.next().await {
                        match subscription_response {
                            Ok(response) => {
                                println!("Initial response: {}", response.to_string());
                            }
                            Err(error) => println!("WS Error: {:?}", error),
                        }
                    }

                    // Handle ongoing stream
                    while let Some(Ok(msg)) = ws_stream.next().await {
                        match serde_json::from_str::<LiveOrderBookResponse>(&msg.to_string()) {
                            Ok(order_book) => {
                                let order_book: Box<dyn OrderBook + Send> = Box::new(order_book);
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

#[derive(Clone, Debug, Serialize)]
struct Channel {
    channel: String,
}

impl Display for Channel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.channel)
    }
}

impl Channel {
    fn new(channel: String) -> Self {
        Self { channel }
    }
}

#[derive(Debug, Serialize)]
struct ChannelSubscriptionRequest {
    event: String,
    data: Channel,
}

impl ChannelSubscriptionRequest {
    fn new(channel: Channel) -> Self {
        Self {
            event: BTS_SUBSCRIBE.to_string(),
            data: channel,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct LiveOrderBookResponse {
    data: LiveOrderBookData,
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
pub(crate) struct LiveOrderBookData {
    #[serde(skip)]
    timestamp: u64,
    #[serde(skip)]
    microtimestamp: u64,
    bids: Vec<Order>,
    asks: Vec<Order>,
    #[serde(skip)]
    channel: String,
    #[serde(skip)]
    event: String,
}

impl OrderBook for LiveOrderBookResponse {
    fn source(&self) -> String {
        BITSTAMP.to_string()
    }

    fn spread(&self) -> f64 {
        self.best_asks(1)[0].price - self.best_bids(1)[0].price
    }

    fn best_asks(&self, depth: usize) -> Vec<Level> {
        best_orders_to_depth(self.data.asks.clone(), Ordering::LowToHigh, depth, BITSTAMP)
    }

    fn best_bids(&self, depth: usize) -> Vec<Level> {
        best_orders_to_depth(self.data.bids.clone(), Ordering::HighToLow, depth, BITSTAMP)
    }
}
