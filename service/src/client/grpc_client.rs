use anyhow::{Context, Error};
use tokio::sync::mpsc::channel as mpsc_channel;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Channel, Request};
use url::Url;

// use shared_types::{
//     proto::{orderbook_aggregator_client::OrderbookAggregatorClient, Empty, Summary},
//     TradedPair,
// };
//
// #[tokio::main]
// async fn main() {
//     let mut client = OrderbookAggregatorClient::connect("http://0.0.0.0:3030")
//         .await
//         .unwrap();
//
//     let _ = get_order_book_for_pair(
//         &mut client,
//         TradedPair::new("ETH".to_string(), "BTC".to_string()),
//     )
//     .await;
// }
//
// async fn get_order_book_for_pair(
//     client: &mut OrderbookAggregatorClient<Channel>,
//     _traded_pair: TradedPair,
// ) -> Result<(), Error> {
//     let mut stream = client
//         .book_summary(Request::new(Empty {}))
//         .await?
//         .into_inner();
//
//     while let Some(summary) = stream.message().await? {
//         println!("Output: {}", summary);
//     }
//
//     Ok(())
// }
//
// pub async fn subscribe_to_ethbtc(server_address: Url) -> Result<ReceiverStream<Summary>, Error> {
//     let (summary_tx, summary_rx) = mpsc_channel(100);
//
//     let mut client = OrderbookAggregatorClient::connect(server_address.to_string())
//         .await
//         .context("Should have connected to grpc server")?;
//
//     let mut stream = client
//         .book_summary(Request::new(Empty {}))
//         .await?
//         .into_inner();
//
//     tokio::spawn(async move {
//         while let Some(summary) = stream.message().await? {
//             let _ = summary_tx.send(summary).await;
//         }
//         Ok::<(), Error>(())
//     });
//
//     Ok(ReceiverStream::new(summary_rx))
// }
fn main() {}
