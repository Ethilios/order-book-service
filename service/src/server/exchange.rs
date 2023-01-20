use std::{fmt::Debug, str::FromStr};

use anyhow::Error;
use serde::{de, Deserialize, Deserializer};
use tokio::sync::mpsc::Receiver;

use shared_types::proto::{Level, TradedPair};

pub(crate) type BoxedOrderbook = Box<dyn OrderBook + Send>;
pub(crate) type BoxedExchange = Box<dyn Exchange + Send>;

impl Clone for BoxedExchange {
    fn clone(&self) -> Self {
        self.clone_dyn()
    }
}

/// [Exchange] is a unified interface which can be applied to any exchange
pub(crate) trait Exchange {
    fn name(&self) -> String;

    fn stream_order_book_for_pair(
        &self,
        traded_pair: &TradedPair,
    ) -> Result<Receiver<BoxedOrderbook>, Error>;

    // This method is required to allow the trait object to be Clone
    fn clone_dyn(&self) -> BoxedExchange;
}

/// [OrderBook] is a unified interface which can be applied to an order book
/// from any exchange regardless of format
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

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Order {
    #[serde(deserialize_with = "type_from_str")]
    pub(crate) price: f64,
    #[serde(deserialize_with = "type_from_str")]
    pub(crate) quantity: f64,
}

impl Order {
    pub(crate) fn new(price: f64, quantity: f64) -> Self {
        Self { price, quantity }
    }
}

// This is only sorting by `price`. To sort orders across multiple exchanges,
// secondary ordering on `amount` should applied to handle the case where two exchanges share a `price` level.
impl PartialOrd for Order {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.price < other.price {
            return Some(std::cmp::Ordering::Less);
        } else if self.price > other.price {
            return Some(std::cmp::Ordering::Greater);
        }
        Some(std::cmp::Ordering::Equal)
    }
}

pub(crate) enum Ordering {
    LowToHigh,
    HighToLow,
}

/// Helper to sort a collection of orders and return a depth-constrained sub-set.
pub(crate) fn sort_orders_to_depth(
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

/// Data returned from exchanges is often stringified, this helper aids in converting these to their Rust types.
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

#[cfg(test)]
mod tests {
    use lazy_static::lazy_static;

    use super::{sort_orders_to_depth, Order, Ordering};
    use shared_types::proto::Level;

    lazy_static! {
        static ref ORDERS_LOW_TO_HIGH: Vec<Order> = vec![
            Order::new(1.0, 1.0),
            Order::new(2.0, 1.0),
            Order::new(3.0, 1.0),
            Order::new(4.0, 1.0),
            Order::new(5.0, 1.0),
            Order::new(6.0, 1.0),
            Order::new(7.0, 1.0),
            Order::new(8.0, 1.0),
            Order::new(9.0, 1.0),
            Order::new(10.0, 1.0),
        ];
        static ref ORDERS_HIGH_TO_LOW: Vec<Order> = vec![
            Order::new(10.0, 1.0),
            Order::new(9.0, 1.0),
            Order::new(8.0, 1.0),
            Order::new(7.0, 1.0),
            Order::new(6.0, 1.0),
            Order::new(5.0, 1.0),
            Order::new(4.0, 1.0),
            Order::new(3.0, 1.0),
            Order::new(2.0, 1.0),
            Order::new(1.0, 1.0),
        ];
    }

    // The function being tested is only used on [Order]s from a single exchange so it is assumed that
    // each `price` level is unique so sorting by quantity is not being tested here.
    #[test]
    fn should_sort_ask_orders_correctly() {
        let expected = ORDERS_LOW_TO_HIGH
            .clone()
            .into_iter()
            .map(|order| Level::new("EXAMPLE", order.price, order.quantity))
            .collect::<Vec<Level>>();

        // For this I've used the opposite sorting to what is expected as the input.
        let actual = sort_orders_to_depth(
            ORDERS_HIGH_TO_LOW.clone(),
            Ordering::LowToHigh,
            10,
            "EXAMPLE",
        );

        assert_eq!(expected, actual);
    }

    #[test]
    fn should_sort_bid_orders_correctly() {
        let expected = ORDERS_HIGH_TO_LOW
            .clone()
            .into_iter()
            .map(|order| Level::new("EXAMPLE", order.price, order.quantity))
            .collect::<Vec<Level>>();

        // For this I've used the opposite sorting to what is expected as the input.
        let actual = sort_orders_to_depth(
            ORDERS_LOW_TO_HIGH.clone(),
            Ordering::HighToLow,
            10,
            "EXAMPLE",
        );

        assert_eq!(expected, actual);
    }
}
