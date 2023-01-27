pub mod proto {
    pub mod orderbook {
        use crate::proto::OrderBookRequest;
        #[cfg(test)]
        use std::collections::hash_map::DefaultHasher;
        use std::{
            cmp::Ordering,
            fmt::{Display, Formatter},
            hash::{Hash, Hasher},
        };
        use tonic::IntoRequest;
        tonic::include_proto!("orderbook");

        // These impl blocks are to allow me to use the generated types from the proto schema.
        // The auto-generated types don't have these traits derived so I need to do it here.

        impl Hash for TradedPair {
            fn hash<H: Hasher>(&self, state: &mut H) {
                self.first.hash(state);
                self.second.hash(state);
            }
        }

        impl Eq for TradedPair {}

        #[test]
        fn hash_traded_pair_should_work() {
            let one_two = TradedPair::new("One", "Two");
            let also_one_two = TradedPair::new("One", "Two");
            let three_four = TradedPair::new("Three", "Four");

            let mut hasher = DefaultHasher::new();
            one_two.hash(&mut hasher);
            let hashed_one_two = hasher.finish();

            let mut hasher = DefaultHasher::new();
            also_one_two.hash(&mut hasher);
            let hash_also_one_two = hasher.finish();

            let mut hasher = DefaultHasher::new();
            three_four.hash(&mut hasher);
            let hash_three_four = hasher.finish();

            assert_eq!(hashed_one_two, hash_also_one_two);
            assert_ne!(hashed_one_two, hash_three_four);
        }

        impl Display for TradedPair {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}-{}", self.first, self.second)
            }
        }

        impl TradedPair {
            pub fn new(first: &'static str, second: &'static str) -> Self {
                TradedPair {
                    first: first.to_string(),
                    second: second.to_string(),
                }
            }

            pub fn symbol_lower(&self) -> String {
                format!("{}{}", self.first, self.second).to_lowercase()
            }
        }

        impl PartialOrd for Level {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                if self.price < other.price {
                    return Some(Ordering::Less);
                } else if self.price > other.price {
                    return Some(Ordering::Greater);
                }

                // If the price is the same order by amount High -> Low
                if self.amount < other.amount {
                    println!("{} is less than {}", self.amount, other.amount);
                    Some(Ordering::Less)
                } else if self.amount > other.amount {
                    Some(Ordering::Greater)
                } else {
                    Some(Ordering::Equal)
                }
            }
        }

        impl Level {
            pub fn new(exchange: &str, price: f64, quantity: f64) -> Self {
                Self {
                    exchange: exchange.to_string(),
                    price,
                    amount: quantity,
                }
            }

            /// This will order the [Level]s Low->High by [price].
            /// Where [price] of `self` and `other` are equal it is then ordered High->Low by [amount]
            pub fn sort_as_asks(&self, other: &Self) -> Option<Ordering> {
                // Compare `price`
                if self.price < other.price {
                    return Some(Ordering::Less);
                } else if self.price > other.price {
                    return Some(Ordering::Greater);
                }

                // The `price` is equal, compare `amount`
                // Note that the comparisons are counter to what is implied by the [Ordering] returned.
                // This is because amount should always be ordered High->Low.
                if self.amount > other.amount {
                    return Some(Ordering::Less);
                } else if self.amount < other.amount {
                    return Some(Ordering::Greater);
                };

                // `price` and `amount` are equal
                Some(Ordering::Equal)
            }

            /// This will order the [Level]s High->Low by [price].
            /// Where [price] of `self` and `other` are equal it is then ordered High->Low by [amount]
            pub fn sort_as_bids(&self, other: &Self) -> Option<Ordering> {
                // Compare `price`
                // Note that the comparisons are counter to what is implied by the [Ordering] returned.
                // This is because `price` is being ordered High->Low.
                if self.price > other.price {
                    return Some(Ordering::Less);
                } else if self.price < other.price {
                    return Some(Ordering::Greater);
                };

                // The `price` is equal, compare `amount`
                // Note that the comparisons are counter to what is implied by the [Ordering] returned.
                // This is because `amount` should always be ordered High->Low.
                if self.amount > other.amount {
                    return Some(Ordering::Less);
                } else if self.amount < other.amount {
                    return Some(Ordering::Greater);
                };

                // `price` and `amount` are equal
                Some(Ordering::Equal)
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
                    result.and_then(|_| writeln!(f, "\t{level},"))
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

        impl IntoRequest<OrderBookRequest> for TradedPair {
            fn into_request(self) -> tonic::Request<OrderBookRequest> {
                tonic::Request::new(self.into())
            }
        }

        impl From<TradedPair> for OrderBookRequest {
            fn from(value: TradedPair) -> Self {
                Self {
                    traded_pair: Some(value),
                }
            }
        }
    }

    // Re-export the types
    pub use orderbook::{
        orderbook_aggregator_client, orderbook_aggregator_server, Empty, Level,
        Request as OrderBookRequest, Summary, TradedPair,
    };
}
