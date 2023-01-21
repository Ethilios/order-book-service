use anyhow::Error;
use std::collections::HashMap;
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
        if let Some(summary_receiver) = map_lock.get(&requested_pair) {
            let new_subscription = summary_receiver.resubscribe();

            // This channel is used between the service producing the Summary and the task that wraps it in a Result
            let (summary_tx, summary_rx) = mpsc_channel(100);

            tokio::spawn(handle_subscription_stream(new_subscription, summary_tx));

            return Ok(Response::new(ReceiverStream::new(summary_rx)));
        }

        // This is the first time the requested pair has been received
        // The server needs to request that the service spins up a new aggregator to start providing Summarys

        let (summary_receiver_tx, summary_receiver_rx) = oneshot_channel();

        let _ = self
            .new_subscriber_notifier
            // todo should this be a borrow - also quite a cheap clone so maybe not an issue
            .send((requested_pair.clone(), summary_receiver_tx))
            .await;

        // Subscribe to the existing Summary channel for the requested traded pair
        let summary_receiver = summary_receiver_rx
            .await
            .map_err(|recv_err| Status::from_error(recv_err.into()))?;

        // Create a new subscription for the client
        let new_subscription = summary_receiver.resubscribe();

        // Push the new receiver to the HashMap of summary receivers
        map_lock.insert(requested_pair, summary_receiver);

        // The receiving side of this channel will be returned to the client as a stream.
        let (summary_tx, summary_rx) = mpsc_channel(100);

        // This task takes the sending side of the summary channel and populates it with Summary events as it receives OrderBooks from the server-side subscription.
        // todo handle duplication
        tokio::spawn(handle_subscription_stream(new_subscription, summary_tx));

        Ok(Response::new(ReceiverStream::new(summary_rx)))
    }
}

pub(crate) async fn start_server(new_subscriber_notifier: NewSubscriberNotifier) {
    // todo make port config value
    let server_addr = "0.0.0.0:3030".parse().unwrap();

    let order_book = OrderbookService {
        new_subscriber_notifier,
        summary_receivers: Mutex::new(HashMap::new()),
    };

    let svc = OrderbookAggregatorServer::new(order_book);

    Server::builder()
        .add_service(svc)
        .serve(server_addr)
        .await
        .expect("Service ended");
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
