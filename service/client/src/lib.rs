use anyhow::{Context, Error};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Status, Streaming};
use url::Url;

use order_book_service_types::proto::{
    orderbook_aggregator_client::OrderbookAggregatorClient, Summary, TradedPair,
};

type SummaryResult = Result<Summary, Status>;

pub struct ConnectionSettings {
    pub server_address: Url,
    pub traded_pair: TradedPair,
    pub max_attempts: usize,
    pub delay_between_attempts: Duration,
}

pub async fn connect_to_summary_service(settings: ConnectionSettings) -> ReceiverStream<Summary> {
    let mut attempts = 0;

    let (summary_tx, summary_rx) = mpsc::channel(300);

    tokio::spawn(async move {
        while attempts < settings.max_attempts {
            attempts += 1;

            match connect_to_server_for_pair(
                settings.server_address.clone(),
                settings.traded_pair.clone(),
            )
            .await
            {
                Ok(mut summary_stream) => {
                    while let Ok(Some(summary)) = summary_stream.message().await {
                        attempts = 0;
                        let _ = summary_tx.send(summary).await;
                    }
                }
                Err(grpc_error) => {
                    println!("Error connecting to server: {grpc_error}");
                    println!("Retrying...\t({attempts}/{})", settings.max_attempts);
                    tokio::time::sleep(settings.delay_between_attempts).await;
                }
            }
        }
    });

    summary_rx.into()
}

async fn connect_to_server_for_pair(
    server_address: Url,
    traded_pair: TradedPair,
) -> Result<Streaming<Summary>, Error> {
    let mut client = OrderbookAggregatorClient::connect(server_address.to_string())
        .await
        .context("Error making initial connection to server")?;

    let orderbook_stream = client
        .book_summary(traded_pair)
        .await
        .context("Error calling the BookSummary RPC")?
        .into_inner();

    Ok(orderbook_stream)
}
