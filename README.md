### order-book-service

## Overview

The service should connect concurrently to websocket feeds from the configured exchanges.
From those feeds it should retrieve order books for the configured traded pair of currencies.
Then the received order books should be merged into a single combined order book.
From this order book the following can be collated:
 - Spread
 - Top 10 bids
 - Top 10 asks

The above should then be made available via a gRPC stream.

## Plan

### Single Connection

- [x] The service should connect to a single exchange, to receive the order book for a given key-pair.
- [x] The service should sort the order book and then extract the top 10 bids and asks.
- [x] The service should calculate the spread from the order book.
- [ ] The service should then publish the summary data to the outbound gRPC stream.

### Adding Configuration
- [ ] The service should be configurable using a `config.toml` to store:
  - Exchange
  - WebSocket Address
  - Traded Pair

### Extending to Multiple Connections

- [ ] The config should be modified to hold a list instead of a single entry.
- [ ] The service should make concurrent attempts to connect to the configured exchanges.
- [ ] The service should merge the received order books and then sort the entries to allow extraction of the top 10 bids and asks.
- [ ] The service should calculate the spread from the combined order book.
- [ ] The service should then publish the summary data to the outbound gRPC stream.

### Presentation

- [ ] A basic frontend could be rapidly mocked up from boilerplate code.
- [ ] The frontend should connect to the Rust gRPC server to get the summary data.
- [ ] The frontend should auto-update as new data is received.

### Future Improvements

- [ ] The service could handle drops in connections with configurable reconnection attempts, delays and timeouts.
- [ ] The service could store the summary data to allow clients to query historic data - e.g. via REST.
- [ ] The frontend could be improved generally but also to implement a frontend to query the historic data described above.