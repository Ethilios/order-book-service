## Orderbook Service
[![CI](https://github.com/Ethilios/order-book-service/actions/workflows/order-book-servicei-ci.yml/badge.svg)](https://github.com/Ethilios/order-book-service/actions/workflows/order-book-servicei-ci.yml)

The orderbook service aggregates real-time data from exchanges to produce summaries.

### Quick Start
Move into the `service` dir:
```shell
cd service
```

Start by spinning up the server:
```shell
RUST_LOG=info cargo run -p "order-book-service-server"
```

<details>
<summary>Example Output</summary>
<pre>
$ cargo run -p "order-book-service-server"
    Finished dev [unoptimized + debuginfo] target(s) in 0.06s
     Running `target/debug/order-book-service-server`
2023-01-27T10:34:32.161905Z  INFO order_book_service_server: Starting orderbook service on port :3030...
</pre>
</details>

Then in another terminal, use the CLI to subscribe to summaries for a traded pair:
```shell
cargo run -p "order-book-service-cli" -- "http://0.0.0.0:3030" "ETH" "BTC"
```
<details>
<summary>Example Output</summary>
<pre>
cargo run -p "order-book-service-cli" -- "http://0.0.0.0:3030" "ETH" "BTC"
   Compiling order-book-service-cli v0.1.0 (/home/george/ethilios/order-book-service/service/cli)
    Finished dev [unoptimized + debuginfo] target(s) in 1.62s
     Running `target/debug/order-book-service-cli 'http://0.0.0.0:3030' ETH BTC`
Orderbook Service CLI
Attempting to connect...        (1/10)
{
        "spread": 0.000001000000000001,
        "asks": [
        { "exchange": Binance, "price": 0.068841, "amount": 19.4395 },
        { "exchange": Binance, "price": 0.068842, "amount": 0.0019 },
        { "exchange": Binance, "price": 0.068843, "amount": 0.0099 },
        { "exchange": Binance, "price": 0.068844, "amount": 0.0019 },
        { "exchange": Binance, "price": 0.068845, "amount": 2.2526 },
        { "exchange": Binance, "price": 0.068846, "amount": 0.0019 },
        { "exchange": Binance, "price": 0.068847, "amount": 2.1802 },
        { "exchange": Binance, "price": 0.068848, "amount": 0.0019 },
        { "exchange": Binance, "price": 0.068849, "amount": 6.3253 },
        { "exchange": Bitstamp, "price": 0.06884927, "amount": 0.4975128 },
],
"bids": [
        { "exchange": Binance, "price": 0.06884, "amount": 18.3602 },
        { "exchange": Binance, "price": 0.068839, "amount": 0.0019 },
        { "exchange": Binance, "price": 0.068838, "amount": 8.9936 },
        { "exchange": Binance, "price": 0.068837, "amount": 6.3265 },
        { "exchange": Binance, "price": 0.068836, "amount": 1.0019 },
        { "exchange": Binance, "price": 0.068835, "amount": 0.0019 },
        { "exchange": Binance, "price": 0.068834, "amount": 0.0019 },
        { "exchange": Binance, "price": 0.068833, "amount": 0.0019 },
        { "exchange": Binance, "price": 0.068832, "amount": 0.0019 },
        { "exchange": Binance, "price": 0.068831, "amount": 0.3435 },
] 
}

</pre>
</details>

### Project Structure
The service is written in Rust and organised in a Cargo workspace, with members:
- `server`
- `client`
- `cli`
- `common`

The server contains the code for connecting to the exchanges, aggregating the orderbooks
and providing the summaries via a gRPC endpoint.
The client library has a single external method for subscribing to the summary endpoint of the server.
The CLI is a simple wrapper for the client.
Common contains the `.proto` schema, it generates the types and exposes them for the client and server to use.

### Server
The server is the backbone of the service. It has a single gRPC endpoint:

------------------------------------------------------------------------------------------

<details>
 <summary>BookSummary</summary>

**URL**: `/`  
**Request**:

```json
{
  "traded_pair": {
    "first": "<Token Symbol>", // e.g. "ETH"
    "second": "<Token Symbol>" // e.g. "BTC"
  }
}
```
**Response**: (Streaming)
```json
{
  "spread": 0.000001000000000001,
  "asks": [
    {
      "exchange": "Binance",
      "price": 0.069591,
      "amount": 5.4281
    },
    //...x10
  ],
  "bids": [
    {
      "exchange": "Binance",
      "price": 0.06959,
      "amount": 25.051
    },
    //...x10
  ]
}
```
</details>

------------------------------------------------------------------------------------------
The main process sets up the exchange instances and then spawns two tasks,
a gRPC server and a request handler.

When the RPC is called the server checks if it has already received a request for the provided `traded_pair`.
- If it's the first time, the `grpc_server` will make a request to the main process to spin up a new aggregator.
  The new aggregator's receiver is then cached in the gRPC server. The server subscribes to the aggregator and streams the responses to the client.
- If it has already handled this token then there will be an existing aggregator and a receiver in the grpc_server's cache.
  This cached receiver is then resubscribed to and streamed to the client.

#### OrderbookAggregator

The `OrderbookAggregator`'s job is to connect to each of it's source exchanges for a given `TradedPair`and merge the incoming orderbooks into a `Summary`.
The `Summary` is then streamed to subscribed receivers.

### Client

The client is quite simple, it has a single public function for connecting to the server's Summary service.
It has a configurable retry loop for connecting to the server.

------------------------------------------------------------------------------------------

<details>
<summary><code>connect_to_summary_service</code></summary>

It takes a single arg (`settings`) to define the connection which specifies the server address to bind to, the desired traded pair,
the maximum no. of attempts that should be made to connect and finally the delay before making a new attempt.
```rust
pub struct ConnectionSettings {
    pub server_address: Url,
    pub traded_pair: TradedPair,
    pub max_attempts: usize,
    pub delay_between_attempts: Duration,
}
```
It returns `ReceiverStream<Result<Summary, Status>>`.

</details>

------------------------------------------------------------------------------------------

### Future Improvements

- The grpc_server could be wrapped in a [Tower](https://docs.rs/tower/latest/tower/) service to allow for rate and concurrency limiting.
- The service could store the summary data to allow clients to query historic data via a new `gRPC` call or `REST` API.
- A frontend app could be written to consume the data via the gRPC server or leveraging the `orderbook-service-client` lib's `ffi`.
