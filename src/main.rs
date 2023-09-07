use clap::Parser;
use orderbook_challenge::orders;

/// Pulls order depths for the given currency pair from the WebSocket feeds of multiple exchanges.
/// Publishes a merged order book as a gRPC stream.
#[derive(Parser)]
struct Cli {
    #[clap(short, long, help = "(Optional) Currency pair to subscribe to. Default: ETH/BTC")]
    currency_pair: Option<String>,

    #[clap(short, long, help = "(Optional) Port number on which the the gRPC server will be hosted. Default: 8000")]
    port: Option<usize>,

    #[clap(long, help = "(Optional) Skip Bitstamp. Default: false")]
    skip_bitstamp: bool,

    #[clap(long, help = "(Optional) Skip Binance. Default: false")]
    skip_binance: bool,

}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Cli::parse();
    // let currency_pair: String = match args.currency_pair {
    //     Some(value) => value,
    //     None => "ETH/BTC".to_string(),
    // };
    
    // let port: usize = match args.port {
    //     Some(value) => value,
    //     None => 8000,
    // };
    let currency_pair: String = args.currency_pair.unwrap_or("ETH/BTC".to_string());
    let port: usize = args.port.unwrap_or(8000);
    let skip_bitstamp: bool = args.skip_bitstamp;
    let skip_binance: bool = args.skip_binance;

    orders::run(&currency_pair, port,
                 skip_bitstamp, skip_binance).await.unwrap();
}
