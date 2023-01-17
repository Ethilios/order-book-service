use tokio::sync::broadcast::Receiver as BroadcastReceiver;
use tokio::sync::mpsc::channel as mpsc_channel;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};

use crate::exchange::OrderBook;
use shared_types::proto::{
    orderbook_aggregator_server::{OrderbookAggregator, OrderbookAggregatorServer},
    Empty, Summary,
};

/// The [OrderbookService]'s role is to emit a stream of Summary data.
/// It does this by receiving a stream of Orderbooks and then parsing out the spread, top 10 asks and top 10 bids.
#[derive(Debug)]
pub(crate) struct OrderbookService {
    summary_receiver: BroadcastReceiver<Summary>,
}

#[tonic::async_trait]
impl OrderbookAggregator for OrderbookService {
    type BookSummaryStream = ReceiverStream<Result<Summary, Status>>;

    #[allow(unused)]
    async fn book_summary(
        &self,
        request: Request<Empty>,
    ) -> Result<Response<Self::BookSummaryStream>, Status> {
        // The server-side subscription
        let mut new_subscriber = self.summary_receiver.resubscribe();

        // The receiving side of this channel will be returned to the client as a stream.
        let (summary_tx, summary_rx) = mpsc_channel(100);

        // This task takes the sending side of the summary channel and populates it with Summary events as it receives OrderBooks from the server-side subscription.
        tokio::spawn(async move {
            while let Ok(summary) = new_subscriber.recv().await {
                // todo convert

                summary_tx.send(Ok(summary)).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(summary_rx)))
    }
}

pub(crate) async fn run_server(summary_receiver: BroadcastReceiver<Summary>) {
    // todo make port config value
    let server_addr = "0.0.0.0:3030".parse().unwrap();

    let order_book = OrderbookService { summary_receiver };

    let svc = OrderbookAggregatorServer::new(order_book);

    Server::builder()
        .add_service(svc)
        .serve(server_addr)
        .await
        .expect("Service ended");
}

// todo build up client library then use here to test the server
#[cfg(test)]
mod test {}
