use crate::error_handling::Error;
use crate::order_collection::{self, OutputData};
use crate::orders::OutTickPair;
use futures::Stream;
use log::info;
use rust_decimal::prelude::ToPrimitive;
use std::pin::Pin;
use std::sync::Arc;
// use tokio::sync::RwLock;
use tonic::{transport::Server, Request, Response, Status};

//the orderbook.rs file created after we compiled proto/orderbook.proto using build.rs and tonic-build gets placed under 
//target/debug/build. To include that code in the source code, we use tonic::include_proto!
pub mod proto {
    tonic::include_proto!("orderbook");
}

pub struct OrderBookService {
    // out_ticks: Arc<RwLock<OutTickPair>>,
    // out_ticks: OutTickPair,
    out_ticks: Arc<OutTickPair>,
}

impl OrderBookService {
    // pub fn new(out_ticks: Arc<RwLock<OutTickPair>>) -> Self {
    //     OrderBookService { out_ticks }
    // pub fn new(out_ticks: OutTickPair) -> Self {
    //     OrderBookService { out_ticks }
    // }

    pub fn new(out_ticks: Arc<OutTickPair>) -> Self {
        OrderBookService { out_ticks }
    }

    pub async fn serve(self, port: usize) -> Result<(), Error>{
        let addr = format!("[::1]:{}", port);
        let addr = addr.parse()?;

        info!("Serving grpc at {}", addr);

        Server::builder()
            .add_service(proto::orderbook_aggregator_server::OrderbookAggregatorServer::new(self))
            .serve(addr)
            .await?;

        Ok(())
    }

    // async fn out_tick(&self) -> OutTick {
    //     let reader = self.out_ticks.read().await;
    //     let out_tick = reader.1.borrow().clone();
    //     out_tick
    // }
}

impl From<OutputData> for proto::Summary {
    fn from(out_tick: OutputData) -> Self {
        let spread = out_tick.spread.to_f64().unwrap();
        let bids: Vec<proto::Level> = to_levels(&out_tick.bids);
        let asks: Vec<proto::Level> = to_levels(&out_tick.asks);

        proto::Summary{ spread, bids, asks }
    }
}

fn to_levels(levels: &Vec<order_collection::Level>) -> Vec<proto::Level> {
    levels.iter()
        .map(|l|
            proto::Level{
                exchange: l.exchange.to_string(),
                price: l.price.to_f64().unwrap(),
                amount: l.amount.to_f64().unwrap(),
            })
        .collect()
}

//this trait is given in the orderbook.rs file. #[tonic::async_trait] allows us to use async methods within a trait.
#[tonic::async_trait]
impl proto::orderbook_aggregator_server::OrderbookAggregator for OrderBookService {

    //type BookSummaryStream is also defined in orderbook.rs. It a trait object. In this case it is a type that has
    // the stream trait from futures implemented. It is a dynamic dispatch since the specific implementation of stream
    //is decided by Rust at runtime using vtables. Stream is implemented for multiple types like ws stream, receiver side of channels.
    // the type of items that the stream carries is Result<proto::Summary, Status>. Also, this type has Send trait implemented that allows sending the stream across threads.
    // the lifetime is 'static which means it lives for as long as the program runs. It is also pinned and a box pointer. 
    type BookSummaryStream =
        Pin<Box<dyn Stream<Item = Result<proto::Summary, Status>> + Send + 'static>>;

    async fn book_summary(
        &self,
        request: Request<proto::Empty>,
    ) -> Result<Response<Self::BookSummaryStream>, Status> {
        info!("Got a request: {:?}", request);
        let _req = request.into_inner();

        // let mut rx_out_ticks = self.out_ticks.read().await.1.clone(); //this read lock was used when we were using RwLock on the watch channel
        
        // taking a reference (called as cloning for receiving end of watch channel) of the receiving end of the watch channel or
        //creating a thread that has a reference to the receiving end of the watch channel.
        let mut rx_out_ticks = self.out_ticks.1.clone();

        let output = async_stream::try_stream! {
            // yield the current value by first taking a reference to it using .borrow() and then cloning it (creating a deep copy of it).
            let out_tick = rx_out_ticks.borrow().clone();
            yield proto::Summary::from(out_tick);

            //awaiting change in values in the receiving side.
            while let Ok(_) = rx_out_ticks.changed().await {
                let out_tick = rx_out_ticks.borrow().clone();
                yield proto::Summary::from(out_tick);
            }
        };

        Ok(Response::new(Box::pin(output) as Self::BookSummaryStream))
    }
}

