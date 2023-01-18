use std::{fmt::Debug, str::FromStr};

use anyhow::Error;
use serde::{de, Deserialize, Deserializer};
use tokio::sync::mpsc::Receiver;

use crate::BoxedOrderbook;
use shared_types::{proto::Level, TradedPair};

// Hashmap to represent Order (Key: Price, Value: Quantity)

#[derive(Clone, Debug, Deserialize, PartialEq, PartialOrd)]
pub(crate) struct Order {
    #[serde(deserialize_with = "type_from_str")]
    pub(crate) price: f64,
    #[serde(deserialize_with = "type_from_str")]
    pub(crate) quantity: f64,
}

pub(crate) fn type_from_str<'de, D, T>(deserializer: D) -> Result<T, D::Error>
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
    /// The name of the exchange that produced the orderbook
    fn source(&self) -> String;
    /// The difference between the best ask and best bid
    fn spread(&self) -> f64;
    /// The best [depth] asks - ordered High -> Low
    fn best_asks(&self, depth: usize) -> Vec<Level>;
    /// The best [depth] bids - ordered Low -> High
    fn best_bids(&self, depth: usize) -> Vec<Level>;
}

pub(crate) enum Ordering {
    LowToHigh,
    HighToLow,
}

pub(crate) fn best_orders_to_depth(
    mut orders: Vec<Order>,
    ordering: Ordering,
    depth: usize,
    exchange: &str,
) -> Vec<Level> {
    match ordering {
        Ordering::LowToHigh => orders.sort_by(|a, b| a.partial_cmp(b).unwrap()),
        Ordering::HighToLow => orders.sort_by(|a, b| b.partial_cmp(a).unwrap()),
    };

    let depth_slice = &orders[..depth];

    depth_slice
        .iter()
        .map(|order| Level {
            exchange: exchange.to_string(),
            price: order.price,
            amount: order.quantity,
        })
        .collect()
}

pub(crate) trait Exchange {
    fn name(&self) -> String;

    fn stream_order_book_for_pair(
        &self,
        traded_pair: &TradedPair,
    ) -> Result<Receiver<BoxedOrderbook>, Error>;
}
