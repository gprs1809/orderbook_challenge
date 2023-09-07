# orderbook_challenge
A Rust program that connects to multiple exchanges (Binance and Bitstamp websocket servers specifically) and publishes a live merged order book (top 10 asks and bids along with the spread) using gRPC. 

The challenge statement was to create a mini Rust project that: 
1. connects to two exchanges' websocket feeds at the same time,
2. pulls order books, using these streaming connections, for a given traded pair of currencies (configurable), from each exchange,
3. merges and sorts the order books to create a combined order book,
4. from the combined book, publishes the spread, top ten bids, and top ten asks, as a stream, through a gRPC server.

The code compiles into 2 binaries called grpc_orderbook_server and grpc_orderbook_client, both specified in the cargo.toml file. To execute the program after compiling with default options, run the following from the command line:
```
cargo run --bin grpc_orderbook_server
```
While the server runs, open a new command line window and run the following:
```
cargo run --bin grpc_orderbook_client
```

This executes the program with the default currency pair (etcbtc) and by default merges both streams from binance and bitstamp. To use a different currency pair, run:
```
cargo run --bin grpc_orderbook_server -- --currency-pair ethusd
```
To skip order details from either Binance or Bitstamp, use the skip-binance or skip-bitstamp flags:
```
cargo run --bin grpc_orderbook_server -- --currency-pair ethusd --skip-binance
```
The results that a grpc clients would see on their command line would look something like below:

![image](https://github.com/gprs1809/orderbook_challenge/assets/79402832/fb9192dc-a335-4113-8fcb-28f1d2f21ae7)

I have to mention that there are several resources online like the Rust Documentation, Rust lang book, some blogs and some code respositories online that have been incredibly useful for me to complete this challenge. My understanding of the Rust language has also improved several folds with this challenge.
