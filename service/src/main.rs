use std::{
    fmt::{Display, Formatter},
    string::ToString,
    time::Duration,
};

use anyhow::{Context, Error};
use futures_util::StreamExt;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async, tungstenite::http::Response, MaybeTlsStream, WebSocketStream,
};
use url::Url;

const ETH: &str = "ETH";
const BTC: &str = "BTC";

/// Represents an exchange with a WebSocket interface.
struct Exchange {
    /// Common name of the exchange, e.g. "Binance".
    name: String,
    /// The base WebSocket endpoint without any filter or query elements.
    root_ws_endpoint: Url,
    /// The pairings that the exchange can provide data for.
    valid_traded_pairs: Vec<TradedPair>,
    /// There is usually a pre-determined set of timings that a client can choose between for setting the frequency of updates from the exchange.
    valid_update_cadences: Vec<Duration>,
    /// There is usually a pre-determined set of levels that a client can choose between for setting the number of orders to retrieve.
    valid_depth_levels: Vec<u16>,
}

impl Exchange {
    fn new(
        name: String,
        root_ws_endpoint: &str,
        valid_traded_pairs: Vec<TradedPair>,
        valid_update_cadences: Vec<Duration>,
        valid_depth_levels: Vec<u16>,
    ) -> Result<Self, Error> {
        let url = Url::parse(root_ws_endpoint)?;

        Ok(Self {
            name,
            root_ws_endpoint: url,
            valid_traded_pairs,
            valid_update_cadences,
            valid_depth_levels,
        })
    }

    async fn stream_order_book_data(
        &self,
        traded_pair: TradedPair,
        depth: u16,
        update_cadence: Duration,
    ) -> Result<
        (
            WebSocketStream<MaybeTlsStream<TcpStream>>,
            Response<Option<Vec<u8>>>,
        ),
        Error,
    > {
        if !self.valid_traded_pairs.contains(&traded_pair) {
            let err_msg = format!("{} is not available to check on {}", traded_pair, self.name);
            return Err(Error::msg(err_msg));
        }

        if !self.valid_update_cadences.contains(&update_cadence) {
            let err_msg = format!(
                "{}ms is not a valid update cadence",
                update_cadence.as_millis()
            );
            return Err(Error::msg(err_msg));
        }

        if !self.valid_depth_levels.contains(&depth) {
            let err_msg = format!("{} is not a valid depth level", depth);
            return Err(Error::msg(err_msg));
        }

        let url_str = format!(
            "{root}/{pair}@depth{depth}@{cadence}ms",
            root = self.root_ws_endpoint,
            pair = traded_pair.symbol_lower(),
            depth = depth,
            cadence = update_cadence.as_millis()
        );
        let ws_url = Url::parse(&url_str).context("Should have parsed valid URI")?;

        println!("Connecting to: {}", ws_url);

        connect_async(&ws_url).await.map_err(Error::from)
    }
}

#[derive(Clone, PartialEq)]
struct TradedPair {
    first: String,
    second: String,
}

impl Display for TradedPair {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.first, self.second)
    }
}

impl TradedPair {
    fn new(first: String, second: String) -> Self {
        TradedPair { first, second }
    }

    fn symbol_lower(&self) -> String {
        format!("{}{}", self.first, self.second).to_lowercase()
    }
}

#[tokio::main]
async fn main() {
    let eth_btc_pair = TradedPair::new(ETH.to_string(), BTC.to_string());

    let binance = Exchange::new(
        "Binance".to_string(),
        " wss://stream.binance.com:9443/ws",
        Vec::from([eth_btc_pair.clone()]),
        Vec::from([100, 1000].map(Duration::from_millis)),
        Vec::from([5, 10, 20]),
    )
    .unwrap();

    println!("Attempting to connect to {}...", binance.name);

    let (mut ws_connection, _) = binance
        .stream_order_book_data(eth_btc_pair, 10, Duration::from_millis(1000))
        .await
        .unwrap();

    while let Some(Ok(msg)) = ws_connection.next().await {
        println!("Received: {}", msg);
    }
}
