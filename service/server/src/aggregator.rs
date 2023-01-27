use std::{collections::HashMap, sync::Arc};

use anyhow::Error;
use futures_util::{stream::SelectAll, StreamExt};
use tokio::sync::broadcast::{channel as broadcast_channel, Sender as BroadcastSender};
use tokio_stream::wrappers::ReceiverStream;

use order_book_service_types::proto::{Summary, TradedPair};

use crate::{
    exchange::{BoxedExchange, BoxedOrderbook},
    grpc_server::SummaryReceiver,
};

type SummarySender = BroadcastSender<Result<Summary, Arc<Error>>>;

pub(crate) struct OrderbookAggregator {
    source_exchanges: Vec<BoxedExchange>,
    traded_pair: TradedPair,
    summary_sender: SummarySender,
}

impl OrderbookAggregator {
    pub(crate) fn new(source_exchanges: &[BoxedExchange], traded_pair: TradedPair) -> Self {
        let (summary_sender, _) = broadcast_channel(100);

        Self {
            source_exchanges: source_exchanges.to_vec(),
            traded_pair,
            summary_sender,
        }
    }

    pub(crate) async fn start(self) {
        // Loop through each source exchange. For each try to connect and get a stream for the desired traded-pair.
        // If the attempt fails retry for a number of times.
        // If successful push the receiver and break out of the retry loop.
        let mut orderbook_stream = SelectAll::new();
        for exchange in self.source_exchanges.iter() {
            let mut attempts = 0;
            let max_attempts = 5;

            while attempts < max_attempts {
                attempts += 1;
                match exchange.stream_order_book_for_pair(&self.traded_pair) {
                    Ok(rx) => {
                        orderbook_stream.push(ReceiverStream::new(rx));
                        break;
                    }
                    Err(err) => {
                        println!("{err}");
                        println!(
                            "Unable to connect to {} for pair {}. Retrying...({}/{})",
                            exchange.name(),
                            &self.traded_pair,
                            attempts,
                            max_attempts
                        )
                    }
                }
            }
        }

        if orderbook_stream.len() < 2 {
            let err_msg = format!(
                "Unable to connect to more than one exchange, aggregation not possible for {}",
                self.traded_pair
            );
            println!("{err_msg}");
            // Inform connected clients of the failure
            let _ = self.summary_sender.send(Err(Arc::new(Error::msg(err_msg))));
            return;
        }

        let mut orderbooks = HashMap::new();

        let mut print_reducer = 0;
        while let Some((orderbook, received)) = orderbook_stream.next().await {
            // Check that there are still more than one exchanges sending orderbooks
            if orderbook_stream.len() < 2 {
                let err_msg = "Exchange disconnected, leaving only one connection - unable to aggregate, exiting";
                println!("Error in aggregator: {err_msg}");
                let _ = self.summary_sender.send(Err(Arc::new(Error::msg(err_msg))));
                return;
            }

            print_reducer += 1;

            if print_reducer == 0 || print_reducer % 7 == 0 {
                println!(
                    "Aggregator for {}, received orderbook from {}",
                    self.traded_pair,
                    orderbook.source()
                );
            }

            orderbooks.insert(orderbook.source(), (orderbook, received));

            // If the buffer has more than one orderbook stored then we can generate a summary - this also clears the map to prevent stale data carrying over.
            if orderbooks.keys().len() > 1 {
                // todo check timestamps are within a specified tolerance

                let summary =
                    merge_orderbooks_into_summary(orderbooks.drain().map(|(_, value)| value.0));

                // Send the summary to all subscribers
                let _ = self.summary_sender.send(Ok(summary));
            }
        }
    }

    /// Subscribe to the aggregator, returns a [SummaryReceiver].
    pub(crate) fn subscribe(&self) -> SummaryReceiver {
        self.summary_sender.subscribe()
    }
}

/// Construct a [Summary] from a collection of [OrderBook]s
pub(crate) fn merge_orderbooks_into_summary(
    orderbooks: impl Iterator<Item = BoxedOrderbook>,
) -> Summary {
    let mut asks = Vec::new();
    let mut bids = Vec::new();

    // Loop through order books extending the above vecs with best asks and bids from each.
    for ob in orderbooks {
        asks.append(&mut ob.best_asks(10));
        bids.append(&mut ob.best_bids(10));
    }

    // Sort the combined asks and bids
    asks.sort_by(|a, b| a.sort_as_asks(b).unwrap());
    bids.sort_by(|a, b| a.sort_as_bids(b).unwrap());

    Summary {
        spread: asks[0].price - bids[0].price,
        asks: asks[..10].to_vec(),
        bids: bids[..10].to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use lazy_static::lazy_static;

    use order_book_service_types::proto::{Level, Summary};

    use crate::{
        aggregator::merge_orderbooks_into_summary,
        exchange::{sort_orders_to_depth, BoxedOrderbook, Order, OrderBook, Ordering},
    };

    struct TestOrderbook {
        id: &'static str,
        asks: Vec<Order>,
        bids: Vec<Order>,
    }

    impl TestOrderbook {
        fn new(id: &'static str, asks: Vec<Order>, bids: Vec<Order>) -> Self {
            Self { id, asks, bids }
        }
    }

    impl OrderBook for TestOrderbook {
        fn source(&self) -> &'static str {
            self.id
        }

        fn spread(&self) -> f64 {
            self.best_asks(1)[0].price - self.best_bids(1)[0].price
        }

        fn best_asks(&self, depth: usize) -> Vec<Level> {
            sort_orders_to_depth(self.asks.clone(), Ordering::LowToHigh, depth, self.source())
        }

        fn best_bids(&self, depth: usize) -> Vec<Level> {
            sort_orders_to_depth(self.bids.clone(), Ordering::HighToLow, depth, self.source())
        }
    }

    lazy_static! {
        static ref ORDERS_WHOLE_LEVELS_AT_ONE: Vec<Order> = vec![
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
        static ref ORDERS_WHOLE_LEVELS_AT_TWO: Vec<Order> = vec![
            Order::new(1.0, 2.0),
            Order::new(2.0, 2.0),
            Order::new(3.0, 2.0),
            Order::new(4.0, 2.0),
            Order::new(5.0, 2.0),
            Order::new(6.0, 2.0),
            Order::new(7.0, 2.0),
            Order::new(8.0, 2.0),
            Order::new(9.0, 2.0),
            Order::new(10.0, 2.0),
        ];
    }

    #[test]
    fn should_sort_ask_levels_correctly() {
        let mut unsorted_levels = vec![
            Level::new("Example", 9.0, 5.0),
            Level::new("Example", 10.0, 4.0),
            Level::new("Example", 10.0, 5.0),
            Level::new("Example", 9.0, 4.0),
        ];

        let expected = vec![
            Level::new("Example", 9.0, 5.0),
            Level::new("Example", 9.0, 4.0),
            Level::new("Example", 10.0, 5.0),
            Level::new("Example", 10.0, 4.0),
        ];

        unsorted_levels.sort_by(|a, b| a.sort_as_asks(b).unwrap());

        // Now sorted
        assert_eq!(unsorted_levels, expected);
    }

    #[test]
    fn should_sort_bid_levels_correctly() {
        let mut unsorted_levels = vec![
            Level::new("Example", 10.0, 4.0),
            Level::new("Example", 9.0, 5.0),
            Level::new("Example", 10.0, 5.0),
            Level::new("Example", 9.0, 4.0),
        ];

        let expected = vec![
            Level::new("Example", 10.0, 5.0),
            Level::new("Example", 10.0, 4.0),
            Level::new("Example", 9.0, 5.0),
            Level::new("Example", 9.0, 4.0),
        ];

        unsorted_levels.sort_by(|a, b| a.sort_as_bids(b).unwrap());

        // Now sorted
        assert_eq!(unsorted_levels, expected);
    }

    #[test]
    fn should_merge_orderbooks_into_summary() {
        let asks_one = ORDERS_WHOLE_LEVELS_AT_ONE.clone();
        let bids_one = ORDERS_WHOLE_LEVELS_AT_ONE.clone();

        let asks_two = ORDERS_WHOLE_LEVELS_AT_TWO.clone();
        let bids_two = ORDERS_WHOLE_LEVELS_AT_TWO.clone();

        let test_orderbook_one = TestOrderbook::new("ONE", asks_one, bids_one);
        let test_orderbook_two = TestOrderbook::new("TWO", asks_two, bids_two);

        let test_orderbooks: Vec<BoxedOrderbook> =
            vec![Box::new(test_orderbook_one), Box::new(test_orderbook_two)];

        let merged_orderbook = merge_orderbooks_into_summary(test_orderbooks.into_iter());

        let expected_summary = Summary {
            // The difference between the best ask (1.5) and the best bid (10.0)
            spread: -9.0,
            // Ordered primarily by price from High->Low and secondarily by amount High->Low
            bids: vec![
                Level::new("TWO", 10.0, 2.0),
                Level::new("ONE", 10.0, 1.0),
                Level::new("TWO", 9.0, 2.0),
                Level::new("ONE", 9.0, 1.0),
                Level::new("TWO", 8.0, 2.0),
                Level::new("ONE", 8.0, 1.0),
                Level::new("TWO", 7.0, 2.0),
                Level::new("ONE", 7.0, 1.0),
                Level::new("TWO", 6.0, 2.0),
                Level::new("ONE", 6.0, 1.0),
            ],
            // Ordered primarily by price from Low->High and secondarily by amount High->Low
            asks: vec![
                Level::new("TWO", 1.0, 2.0),
                Level::new("ONE", 1.0, 1.0),
                Level::new("TWO", 2.0, 2.0),
                Level::new("ONE", 2.0, 1.0),
                Level::new("TWO", 3.0, 2.0),
                Level::new("ONE", 3.0, 1.0),
                Level::new("TWO", 4.0, 2.0),
                Level::new("ONE", 4.0, 1.0),
                Level::new("TWO", 5.0, 2.0),
                Level::new("ONE", 5.0, 1.0),
            ],
        };

        assert_eq!(merged_orderbook, expected_summary);
    }
}
