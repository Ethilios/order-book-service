mod exchange;
mod exchanges;

use std::string::ToString;

use exchange::{Exchange, OrderBook, TradedPair};
use exchanges::binance::Binance;

const ETH: &str = "ETH";
const BTC: &str = "BTC";

#[tokio::main]
async fn main() {
    let eth_btc_pair = TradedPair::new(ETH.to_string(), BTC.to_string());

    let binance = Binance::new();

    let mut recv = binance.stream_order_book_for_pair(&eth_btc_pair).unwrap();

    let mut reducer = 5;

    while let Some(msg) = recv.recv().await {
        if reducer % 5 == 0 {
            println!(
                "Spread: {}\nBest Asks: {:?}\nBest Bids: {:?}\n",
                msg.spread(),
                msg.best_asks(5),
                msg.best_bids(5)
            );
        }
        reducer += 1;
    }

    // let exchanges = vec![binance];

    // let ws_connections = join_all(exchanges.iter().map(|exchange| async {
    //     let (ws_connection, _) = exchange
    //         .stream_order_book_for_pair(&eth_btc_pair)
    //
    //         .unwrap();
    //
    //     ws_connection
    // }))
    // .await;
    //
    // let mut exchange_handles = Vec::new();
    //
    // for mut ws_connection in ws_connections {
    //     let join_handle = tokio::spawn(async move {
    //         while let Some(Ok(msg)) = ws_connection.next().await {
    //             println!("Received: {}", msg);
    //         }
    //     });
    //
    //     exchange_handles.push(join_handle);
    // }
    //
    // futures::future::join_all(exchange_handles).await;
}
