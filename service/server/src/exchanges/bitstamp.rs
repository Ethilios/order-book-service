use std::fmt::{Display, Formatter};

use anyhow::Error;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

use crate::exchange::{
    sort_orders_to_depth, BoxedExchange, BoxedOrderbook, Exchange, Order, OrderBook, Ordering,
};
use order_book_service_types::proto::{Level, TradedPair};

const BITSTAMP: &str = "Bitstamp";
const BITSTAMP_WSS_URL: &str = "wss://ws.bitstamp.net";
const BTS_SUBSCRIBE: &str = "bts:subscribe";
const ORDERBOOK_CHANNEL: &str = "order_book_";

#[derive(Clone)]
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
    ) -> Result<Receiver<BoxedOrderbook>, Error> {
        if !VALID_PAIRS.contains(&&*traded_pair.symbol_lower()) {
            return Err(Error::msg(
                "Requested traded pair is not supported by Bitstamp",
            ));
        }

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
                                println!("BITSTAMP ::Initial response: {}", response.to_string());
                            }
                            Err(error) => println!("WS Error: {:?}", error),
                        }
                    }

                    // Handle ongoing stream
                    while let Some(Ok(msg)) = ws_stream.next().await {
                        match serde_json::from_str::<LiveOrderBookResponse>(&msg.to_string()) {
                            Ok(order_book) => {
                                let order_book: BoxedOrderbook = Box::new(order_book);
                                let _ = order_book_tx.send(order_book).await;
                            }
                            Err(serde_err) => {
                                if msg.is_ping() {
                                    println!("Bitstamp sent ping");
                                } else {
                                    println!("\nSerde Error:\n{}", serde_err)
                                }
                            }
                        }
                    }
                }
                Err(ws_err) => println!("\nWebsocket Error:\n{:?}", ws_err),
            }
        });

        Ok(order_book_rx)
    }

    fn clone_dyn(&self) -> BoxedExchange {
        Box::new(self.clone())
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
        sort_orders_to_depth(self.data.asks.clone(), Ordering::LowToHigh, depth, BITSTAMP)
    }

    fn best_bids(&self, depth: usize) -> Vec<Level> {
        sort_orders_to_depth(self.data.bids.clone(), Ordering::HighToLow, depth, BITSTAMP)
    }
}

// This has been taken from https://www.bitstamp.net/websocket/v2/
// The issue is that regardless of what is requested Bitstamp seems to return a success message followed by an empty stream.
// So I've added a short-term solution: a hard-coded list of the supported traded pairs which can be use to check requested pairs.
const VALID_PAIRS: [&str; 175] = [
    "btcusd", "btceur", "btcgbp", "btcpax", "gbpusd", "gbpeur", "eurusd", "xrpusd", "xrpeur",
    "xrpbtc", "xrpgbp", "ltcbtc", "ltcusd", "ltceur", "ltcgbp", "ethbtc", "ethusd", "etheur",
    "ethgbp", "ethpax", "bchusd", "bcheur", "bchbtc", "paxusd", "xlmbtc", "xlmusd", "xlmeur",
    "xlmgbp", "linkusd", "linkeur", "linkgbp", "linkbtc", "omgusd", "omgeur", "omggbp", "omgbtc",
    "usdcusd", "usdceur", "btcusdc", "ethusdc", "eth2eth", "aaveusd", "aaveeur", "aavebtc",
    "batusd", "bateur", "umausd", "umaeur", "daiusd", "kncusd", "knceur", "mkrusd", "mkreur",
    "zrxusd", "zrxeur", "gusdusd", "algousd", "algoeur", "algobtc", "audiousd", "audioeur",
    "audiobtc", "crvusd", "crveur", "snxusd", "snxeur", "uniusd", "unieur", "unibtc", "yfiusd",
    "yfieur", "compusd", "compeur", "grtusd", "grteur", "lrcusd", "lrceur", "usdtusd", "usdteur",
    "usdcusdt", "btcusdt", "ethusdt", "xrpusdt", "eurteur", "eurtusd", "flrusd", "flreur",
    "manausd", "manaeur", "maticusd", "maticeur", "sushiusd", "sushieur", "chzusd", "chzeur",
    "enjusd", "enjeur", "hbarusd", "hbareur", "alphausd", "alphaeur", "axsusd", "axseur",
    "sandusd", "sandeur", "storjusd", "storjeur", "adausd", "adaeur", "adabtc", "fetusd", "feteur",
    "sklusd", "skleur", "slpusd", "slpeur", "sxpusd", "sxpeur", "sgbusd", "sgbeur", "avaxusd",
    "avaxeur", "dydxusd", "dydxeur", "ftmusd", "ftmeur", "shibusd", "shibeur", "ampusd", "ampeur",
    "ensusd", "enseur", "galausd", "galaeur", "perpusd", "perpeur", "wbtcbtc", "ctsiusd",
    "ctsieur", "cvxusd", "cvxeur", "imxusd", "imxeur", "nexousd", "nexoeur", "antusd", "anteur",
    "godsusd", "godseur", "radusd", "radeur", "bandusd", "bandeur", "injusd", "injeur", "rlyusd",
    "rlyeur", "rndrusd", "rndreur", "vegausd", "vegaeur", "1inchusd", "1incheur", "solusd",
    "soleur", "apeusd", "apeeur", "mplusd", "mpleur", "dotusd", "doteur", "nearusd", "neareur",
    "dogeusd", "dogeeur",
];
