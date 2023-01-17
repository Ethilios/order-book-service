use crate::proto::Level;
use std::fmt::{Display, Formatter};

pub mod proto {
    pub mod orderbook {
        use std::cmp::Ordering;
        use std::fmt::{Display, Formatter};
        tonic::include_proto!("orderbook");

        impl PartialOrd for Level {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                if self.price < other.price {
                    return Some(Ordering::Less);
                } else if self.price > other.price {
                    return Some(Ordering::Greater);
                }

                // If the price is the same order by amount High -> Low
                return if self.amount < other.amount {
                    Some(Ordering::Less)
                } else if self.amount > other.amount {
                    Some(Ordering::Greater)
                } else {
                    Some(Ordering::Equal)
                };
            }
        }

        impl Display for Level {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "{{ \"exchange\": {}, \"price\": {}, \"amount\": {} }}",
                    self.exchange, self.price, self.amount
                )
            }
        }

        struct Levels(Vec<Level>);

        impl Display for Levels {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                writeln!(f, "[")?;
                self.0.iter().fold(Ok(()), |result, level| {
                    result.and_then(|_| writeln!(f, "\t{},", level))
                })?;
                write!(f, "]")
            }
        }

        impl From<&Vec<Level>> for Levels {
            fn from(value: &Vec<Level>) -> Self {
                Self(value.clone())
            }
        }

        impl Display for Summary {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(
                    f,
                    "{{\n\t\"spread\": {},\n\t\"asks\": {},\n\"bids\": {} \n}}",
                    self.spread,
                    Levels::from(&self.asks),
                    Levels::from(&self.bids)
                )
            }
        }
    }

    pub use orderbook::{
        orderbook_aggregator_client, orderbook_aggregator_server, Empty, Level, Summary,
    };
}

#[derive(Clone, PartialEq)]
pub struct TradedPair {
    first: String,
    second: String,
}

impl Display for TradedPair {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.first, self.second)
    }
}

impl TradedPair {
    pub fn new(first: String, second: String) -> Self {
        TradedPair { first, second }
    }

    pub fn symbol_lower(&self) -> String {
        format!("{}{}", self.first, self.second).to_lowercase()
    }
}
