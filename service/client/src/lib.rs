extern crate core;

use std::time::Duration;

use anyhow::{Context, Error};
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
            println!(
                "Attempting to connect...\t({attempts}/{})",
                settings.max_attempts
            );

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
                    eprintln!("Error connecting to server: {grpc_error}");
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

pub mod ffi {
    use std::{ffi::CStr, sync::Mutex, time::Duration};

    use libc::{c_char, c_double, c_int, size_t};
    use once_cell::sync::Lazy;
    use tokio::runtime::Runtime;
    use tokio_stream::StreamExt;
    use url::Url;

    use order_book_service_types::proto::{Level, TradedPair};

    use crate::ConnectionSettings;

    static RUNTIME: Lazy<Mutex<Option<Runtime>>> = Lazy::new(|| Mutex::new(None));

    #[repr(C)]
    pub struct CLevel {
        exchange: *const c_char,
        price: c_double,
        amount: c_double,
    }

    #[repr(C)]
    pub struct CSummary {
        spread: c_double,
        bids: *const CLevel,
        bids_length: size_t,
        asks: *const CLevel,
        asks_length: size_t,
    }

    fn setup_runtime() {
        let mut runtime = RUNTIME.lock().expect("Should lock");
        *runtime = Some(Runtime::new().expect("Should create tokio runtime"));
    }

    fn teardown_runtime() {
        let mut runtime = RUNTIME.lock().expect("should lock");
        *runtime = None;
    }

    #[allow(clippy::missing_safety_doc)]
    pub unsafe extern "C" fn connect_to_summary_service(
        server_address: *const c_char,
        token_one_symbol: *const c_char,
        token_two_symbol: *const c_char,
        max_attempts: c_int,
        delay_between_attempts_millis: c_int,
        callback: extern "C" fn(*const CSummary),
    ) -> c_int {
        setup_runtime();

        if let Some(runtime) = RUNTIME.lock().expect("Should lock").as_mut() {
            let server_address_str =
                convert_to_string(server_address).expect("Should convert to string");
            let url = Url::parse(server_address_str).expect("Should parse url");
            let traded_pair = TradedPair::new(
                convert_to_string(token_one_symbol).expect("Should convert to string"),
                convert_to_string(token_two_symbol).expect("Should convert to string"),
            );
            let max_attempts = max_attempts as usize;
            let delay_between_attempts =
                Duration::from_millis(delay_between_attempts_millis as u64);

            let connection_settings = ConnectionSettings {
                server_address: url,
                traded_pair,
                max_attempts,
                delay_between_attempts,
            };

            runtime.block_on(async move {
                let mut recv_stream = super::connect_to_summary_service(connection_settings).await;

                while let Some(Ok(summary)) = recv_stream.next().await {
                    let c_level_bids = summary
                        .bids
                        .into_iter()
                        .map(level_to_clevel)
                        .collect::<Vec<CLevel>>();

                    let c_level_asks = summary
                        .asks
                        .into_iter()
                        .map(level_to_clevel)
                        .collect::<Vec<CLevel>>();

                    let c_summary = CSummary {
                        spread: summary.spread as c_double,
                        bids: c_level_bids.as_ptr(),
                        bids_length: c_level_bids.len(),
                        asks: c_level_asks.as_ptr(),
                        asks_length: c_level_asks.len(),
                    };

                    callback(&c_summary as *const CSummary)
                }
            });

            teardown_runtime();
            0
        } else {
            teardown_runtime();
            3
        }
    }

    fn level_to_clevel(mut level: Level) -> CLevel {
        // Append nul character for CStr
        level.exchange.push(0.into());

        CLevel {
            exchange: CStr::from_bytes_with_nul(level.exchange.as_ref())
                .expect("Should parse to CString")
                .as_ptr(),
            price: level.price,
            amount: level.amount,
        }
    }

    unsafe fn convert_to_string(c_string: *const c_char) -> Result<&'static str, u8> {
        if c_string.is_null() {
            return Err(1);
        }
        let raw = CStr::from_ptr(c_string);
        let str = match raw.to_str() {
            Ok(s) => s,
            Err(_) => return Err(1),
        };

        Ok(str)
    }
}
