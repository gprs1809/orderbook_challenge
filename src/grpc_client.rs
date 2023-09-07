use clap::Parser;
use proto::orderbook_aggregator_client::OrderbookAggregatorClient;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use rust_decimal_macros::dec;
// use crossterm::{cursor, execute};
// use crossterm::cursor::MoveTo;
// use crossterm::terminal::{Clear, ClearType};
// use crossterm::{ExecutableCommand, terminal};
// use std::io::stdout;


mod proto {
    tonic::include_proto!("orderbook");
}

#[derive(Parser)]
struct Cli {
    #[clap(short, long, help = "(Optional) Port number of the gRPC server. Default: 8000")]
    port: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args = Cli::parse();
    let port: usize = args.port.unwrap_or(8000);
    let addr = format!("http://[::1]:{}", port);

    let mut client = OrderbookAggregatorClient::connect(addr).await?;
    let request = tonic::Request::new(proto::Empty {});

    println!("Receiving updates from gRPC server...");

    let mut response = client.book_summary(request).await?.into_inner();


    // listening to stream
    while let Some(res) = response.message().await? {

        let proto::Summary{spread, bids, asks} = res;

        // print spread
        let mut spread = Decimal::from_f64(spread).unwrap();
        spread.rescale(8);
        spread_percentage(spread, asks.first())
            .map(|perc|
                println!("Spread: {} ({}%)", spread, perc)
            );

        // print bids
        println!("Bids:");
        for bid in &bids {
            let price = Decimal::from_f64(bid.price).unwrap();
            let amount = Decimal::from_f64(bid.amount).unwrap();
            println!("Price: {}, Amount: {}, Exchange: {}", price, amount, bid.exchange);
        }

        // print asks
        println!("Asks:");
        for ask in &asks {
            let price = Decimal::from_f64(ask.price).unwrap();
            let amount = Decimal::from_f64(ask.amount).unwrap();
            println!("Price: {}, Amount: {}, Exchange: {}", price, amount, ask.exchange);
        }
    }

    Ok(())
}

fn spread_percentage(spread: Decimal, best_ask: Option<&proto::Level>) -> Option<Decimal> {
    best_ask.map(|l| {
        let mut perc = spread / Decimal::from_f64(l.price).unwrap() * dec!(100);
        perc.rescale(4);
        perc
    })
}