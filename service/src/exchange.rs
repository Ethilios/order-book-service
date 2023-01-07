use std::{
    fmt::{Debug, Display, Formatter},
    str::FromStr,
};

use anyhow::Error;
use serde::{de, Deserialize, Deserializer};
use tokio::sync::mpsc::Receiver;

#[derive(Clone, PartialEq)]
pub(crate) struct TradedPair {
    first: String,
    second: String,
}

impl Display for TradedPair {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.first, self.second)
    }
}

impl TradedPair {
    pub(crate) fn new(first: String, second: String) -> Self {
        TradedPair { first, second }
    }

    pub(crate) fn symbol_lower(&self) -> String {
        format!("{}{}", self.first, self.second).to_lowercase()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, PartialOrd)]
pub(crate) struct Order {
    #[serde(deserialize_with = "type_from_str")]
    pub(crate) price: f32,
    #[serde(deserialize_with = "type_from_str")]
    pub(crate) quantity: f64,
}

fn type_from_str<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Debug,
{
    let s = <&str>::deserialize(deserializer)?;
    s.parse::<T>().map_err(|from_str_err| {
        let err = format!("{:?}", from_str_err);
        de::Error::custom(err)
    })
}

pub(crate) trait OrderBook {
    fn spread(&self) -> f32;
    fn best_asks(&self, depth: usize) -> Vec<Order>;
    fn best_bids(&self, depth: usize) -> Vec<Order>;
}

pub(crate) trait Exchange<Ob>
where
    Ob: OrderBook,
{
    fn stream_order_book_for_pair(&self, traded_pair: &TradedPair) -> Result<Receiver<Ob>, Error>;
}
