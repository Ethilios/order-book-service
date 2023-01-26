use anyhow::{Context, Error};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::sync::{
    broadcast::Receiver as BroadcastReceiver,
    mpsc::{channel as mpsc_channel, Sender as MpscSender},
    oneshot::{channel as oneshot_channel, Sender as OneshotSender},
    Mutex,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};

use order_book_service_types::proto::{
    orderbook_aggregator_server::{OrderbookAggregator, OrderbookAggregatorServer},
    OrderBookRequest, Summary, TradedPair,
};

pub(crate) type SummaryReceiver = BroadcastReceiver<Result<Summary, Arc<Error>>>;
type NewSubscriberNotifier = MpscSender<(TradedPair, OneshotSender<SummaryReceiver>)>;

/// The [OrderbookService]'s role is to emit a stream of Summary data.
/// It does this by receiving a stream of Orderbooks and then parsing out the spread, top 10 asks and top 10 bids.
#[derive(Debug)]
pub(crate) struct OrderbookService {
    new_subscriber_notifier: NewSubscriberNotifier,
    // Because the auto-generated trait signature for book_summary() takes `&self` not `&mut self` there needs to be a Mutex to guard the HashMap.
    summary_receivers: Mutex<HashMap<TradedPair, SummaryReceiver>>,
}

#[tonic::async_trait]
impl OrderbookAggregator for OrderbookService {
    type BookSummaryStream = ReceiverStream<Result<Summary, Status>>;

    /// This fn is called every time a client hits the BookSummary rpc.
    async fn book_summary(
        &self,
        request: Request<OrderBookRequest>,
    ) -> Result<Response<Self::BookSummaryStream>, Status> {
        let requested_pair = request
            .into_inner()
            .traded_pair
            .ok_or(Status::invalid_argument(
                "This RPC requires traded_pair to be provided",
            ))?;

        // Acquire a lock on the HashMap of receivers
        let mut map_lock = self.summary_receivers.lock().await;

        // There is already a channel for the requested traded pair
        if let Some(existing_summary_receiver) = map_lock.get(&requested_pair) {
            let new_subscription = existing_summary_receiver.resubscribe();

            // This channel is used between the service producing the Summary and the task that wraps it in a Result
            let (summary_tx, summary_rx) = mpsc_channel(100);

            tokio::spawn(handle_subscription_stream(new_subscription, summary_tx));

            return Ok(Response::new(ReceiverStream::new(summary_rx)));
        }

        // This is the first time the requested pair has been received
        // The server needs to request that the service spins up a new aggregator to start providing Summarys

        let (new_request_tx, new_request_rx) = oneshot_channel();

        let _ = self
            .new_subscriber_notifier
            .send((requested_pair.clone(), new_request_tx))
            .await;

        // Subscribe to the existing Summary channel for the requested traded pair or return the Status for the Err case
        let summary_receiver = new_request_rx
            .await
            .map_err(|recv_err| Status::from_error(recv_err.into()))?;

        // Create a new subscription for the client
        let new_subscription = summary_receiver.resubscribe();

        // Push the new receiver to the HashMap of summary receivers
        map_lock.insert(requested_pair, summary_receiver);

        drop(map_lock);

        // The receiving side of this channel will be returned to the client as a stream.
        let (client_channel_tx, client_channel_rx) = mpsc_channel(100);

        // This task takes the sending side of the summary channel and populates it with Summary events as it receives OrderBooks from the server-side subscription.
        tokio::spawn(handle_subscription_stream(
            new_subscription,
            client_channel_tx,
        ));

        Ok(Response::new(ReceiverStream::new(client_channel_rx)))
    }
}

pub(crate) async fn start_server(
    new_subscriber_notifier: NewSubscriberNotifier,
    port: u16,
) -> Result<(), Error> {
    let server_addr = SocketAddr::from(([0, 0, 0, 0], port));

    let order_book = OrderbookService {
        new_subscriber_notifier,
        summary_receivers: Mutex::new(HashMap::new()),
    };

    let svc = OrderbookAggregatorServer::new(order_book);

    Server::builder()
        .add_service(svc)
        .serve(server_addr)
        .await
        .context("gRPC server shutdown")
}

async fn handle_subscription_stream(
    mut rx: SummaryReceiver,
    tx: MpscSender<Result<Summary, Status>>,
) {
    while let Ok(summary_res) = rx.recv().await {
        match summary_res {
            Ok(summary) => {
                let _ = tx.send(Ok(summary)).await;
            }
            Err(err) => {
                // todo error could matched on here to return a more helpful Status
                let _ = tx.send(Err(Status::internal(err.to_string()))).await;
            }
        }
    }
    let _ = tx
        .send(Err(Status::unavailable(
            "The service failed to provide a response",
        )))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast::channel as broadcast_channel;
    use tonic::Code;

    #[tokio::test]
    async fn should_return_summary() {
        let (summary_tx, summary_rx) = broadcast_channel(100);
        let (fn_output_tx, mut fn_output_rx) = mpsc_channel(100);

        let _ = summary_tx.send(Ok(Summary {
            spread: 1.0,
            bids: vec![],
            asks: vec![],
        }));

        // The sender needs to be dropped otherwise the handler will wait for more messages to be sent
        drop(summary_tx);

        handle_subscription_stream(summary_rx, fn_output_tx).await;

        let summary = fn_output_rx
            .recv()
            .await
            .expect("Expected a response from the handler")
            .expect("Expected an Ok(Summary) to be returned from the handler.");

        assert_eq!(summary.spread, 1.0)
    }

    #[tokio::test]
    async fn should_return_status_due_to_internal_error() {
        let (summary_tx, summary_rx) = broadcast_channel(100);
        let (fn_output_tx, mut fn_output_rx) = mpsc_channel(100);

        let _ = summary_tx.send(Err(Arc::new(Error::msg(
            "Internal error, e.g. aggregator couldn't connect",
        ))));

        // The sender needs to be dropped otherwise the handler will wait for more messages to be sent
        drop(summary_tx);

        handle_subscription_stream(summary_rx, fn_output_tx).await;

        let status = fn_output_rx
            .recv()
            .await
            .expect("Expected a response from the handler")
            .expect_err("Expected an Err to be returned from the handler.");

        let expected_status = Status::new(
            Code::Internal,
            "Internal error, e.g. aggregator couldn't connect",
        );

        assert_eq!(status.code(), expected_status.code());
        assert_eq!(status.message(), expected_status.message());
    }

    #[tokio::test]
    async fn should_return_status_at_end_of_stream() {
        let (_, empty_rx) = broadcast_channel(100);
        let (fn_output_tx, mut fn_output_rx) = mpsc_channel(100);

        handle_subscription_stream(empty_rx, fn_output_tx).await;

        let status = fn_output_rx
            .recv()
            .await
            .expect("Expected a response from the handler")
            .expect_err("Expected an Err(Status) to be returned from the handler.");

        let expected_status = Status::new(
            Code::Unavailable,
            "The service failed to provide a response",
        );

        assert_eq!(status.code(), expected_status.code());
        assert_eq!(status.message(), expected_status.message())
    }
}
