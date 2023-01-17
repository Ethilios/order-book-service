// use crate::exchange::OrderBook;
// use shared_types::proto::Summary;
// use std::collections::BTreeMap;
// use tokio::sync::broadcast::Sender;
// use tokio_stream::wrappers::ReceiverStream;
//
// pub(crate) struct OrderbookWithSource {
//     orderbook: Box<dyn OrderBook + Send>,
//     // This is the exchange, I may change it to be an enum
//     source: String,
// }
//
// pub(crate) struct Aggregator {
//     inbound: ReceiverStream<Box<dyn OrderBook + Send>>,
//     outbound: Sender<Summary>,
// }
//
// impl Aggregator {
//     pub(crate) fn add_stream(&mut self, new_stream: ReceiverStream<Box<dyn OrderBook + Send>>) {
//         self.inbound.into_inner();
//     }
// }
