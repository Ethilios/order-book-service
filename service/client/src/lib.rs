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

/// Sets out how the client should connect to the service.  
/// If the client is unable to connect then it will act according to the below:
/// - `max_attempts` is how many times the client should attempt to connect.
/// - `delay_between_attempts` is how long to wait before making a new attempt to connect.
pub struct ConnectionSettings {
    pub server_address: Url,
    pub traded_pair: TradedPair,
    pub max_attempts: usize,
    pub delay_between_attempts: Duration,
}

/// Connect to the service, returning a Stream of [Summary]s (or [Status] in the Err case).
/// Will make repeated attempts to connect as per the [`settings`](ConnectionSettings) provided.  
///
/// Once the internal sender hangs up or the `max_attempts` are exhausted, an error status is sent to the client receiver.
pub async fn connect_to_summary_service(
    settings: ConnectionSettings,
) -> ReceiverStream<SummaryResult> {
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
                Ok(mut summary_stream) => loop {
                    let msg_result = summary_stream.message().await;
                    match msg_result {
                        Ok(Some(summary)) => {
                            attempts = 0;
                            let _ = summary_tx.send(Ok(summary)).await;
                        }
                        Ok(None) => {
                            // Ok(None) means the sender has closed the connection
                            break;
                        }
                        Err(status) => {
                            println!("Received status: {status}");
                            let _ = summary_tx.send(Err(status)).await;
                        }
                    }
                },
                Err(grpc_error) => {
                    println!("Error connecting to server: {grpc_error}");
                    println!("Retrying...\t({attempts}/{})", settings.max_attempts);
                    tokio::time::sleep(settings.delay_between_attempts).await;
                }
            }
        }

        let _ = summary_tx
            .send(Err(Status::unavailable("The service is unavailable")))
            .await;
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
