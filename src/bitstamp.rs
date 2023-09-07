use crate::error_handling::Error;
use crate::order_collection::{self, Exchange, InputData, ToLevel, ToLevels, ToTick};
use crate::websocket;
use futures::SinkExt;
use log::{debug, info};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tungstenite::protocol::Message;

const BITSTAMP_WS_URL: &str = "wss://ws.bitstamp.net";

//the Event struct is created based on the order data from Bitstamp (like the example used in the testing below) and
// also the subscription message sent to bitstamp post connection. Here, I haven't considered unsubscription.
// A typical subscription message sent to bitstamp looks like below:
//{
    //         "event": "bts:subscribe",
    //         "data": {
    //             "channel": "order_book_ethbtc"
    //         }
//the order details from bitstamp (eg used in the testing) has event='data' 
//Debug trait is useful when we want to use the debug macro (debug!) and "{:?}" for printing.
//Deserialize and Serialize are traits from serde crate (which helps in json/string -> Rust objects (deserialize) and Rust objects -> json/string (serialize))
//Deserialize and Serialize are needed for our implemention of deserialize and serialize functions.   
//PartialEq is useful in testing where we use assert_eq. It takes comparison with NaN into account.
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "event")]
enum Event {
    #[serde(rename = "data")]
    Data{data: InData, channel: String},

    #[serde(rename = "bts:subscribe")]
    Subscribe{data: OutSubscription},

    #[serde(rename = "bts:subscription_succeeded")]
    SubscriptionSucceeded{data: InSubscription, channel: String},

}

impl ToTick for Event {
    /// Converts the `Event` into a `Option<InTick>`. Only keep the top ten levels of bids and asks.
    fn option_intick(&self) -> Option<InputData> {
        match self {
            Event::Data { data, .. } => {
                let bids = data.bids.to_levels(order_collection::Side::Bid, 10);
                let asks = data.asks.to_levels(order_collection::Side::Ask, 10);

                Some(InputData { exchange: Exchange::Bitstamp, bids, asks })
            },
            _ => None,
        }
    }
}


#[derive(Debug, PartialEq, Deserialize, Serialize)]
struct OutSubscription {
    channel: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct InData {
    //the order data also has timestamp and microtimestamp (example in testing), but I have ignored them.

    bids: Vec<Level>,
    asks: Vec<Level>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct InSubscription {}

//We need to derive the Clone trait here because in grpc_server.rs, 
//the watch channel passes data of the type OutputData within it. In book_summary method inside a trait, we are referencing
//the latest OutputData from the receiving end (using .borrow) and then cloning it to get ownership or the value itself.
//OutputData struct has fields of the type vec<level> where Level struct is also defined in order_collection
//We convert the Level here into the Level defined there using ToLevel trait and therefore this Level requires clone since OutputData will be cloned.
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
struct Level {
    price: Decimal,
    amount: Decimal,
}

impl ToLevel for Level {
    /// Converts a bitstamp::Level into a order_collection::Level.
    fn to_level(&self, side: order_collection::Side) -> order_collection::Level {
        order_collection::Level::new(side, self.price, self.amount, Exchange::Bitstamp)
    }
}


pub async fn bitstamp_connect(currency_pair: &String) -> Result<websocket::WsStream, Error> {
    let mut ws_stream = websocket::connect(BITSTAMP_WS_URL).await?;
    subscribe(&mut ws_stream, currency_pair).await?;
    Ok(ws_stream)
}

pub fn parse(msg: Message) -> Result<Option<InputData>, Error> {
    let e = match msg {
        Message::Binary(x) => { info!("binary {:?}", x); None },
        Message::Text(x) => {
            debug!("{:?}", x);

            let e= deserialize(x)?;
            match e {
                Event::Data{..} => debug!("{:?}", e),
                _ => info!("{:?}", e),
            }

            Some(e)
        },
        Message::Ping(x) => { info!("Ping {:?}", x); None },
        Message::Pong(x) => { info!("Pong {:?}", x); None },
        Message::Close(x) => { info!("Close {:?}", x); None },
        Message::Frame(x) => { info!("Frame {:?}", x); None },
    };
    Ok(e.map(|e| e.option_intick()).flatten())
}

async fn subscribe (
    rx: &mut websocket::WsStream,
    symbol: &String,
) -> Result<(), Error>
{
    let symbol = symbol.to_lowercase().replace("/", "");
    let channel = format!("order_book_{}", symbol);
    let msg = serialize(Event::Subscribe{ data: OutSubscription { channel } })?;
    rx.send(Message::Text(msg)).await?;
    Ok(())
}

fn deserialize(s: String) -> serde_json::Result<Event> {
    Ok(serde_json::from_str(&s)?)
}

fn serialize(e: Event) -> serde_json::Result<String> {
    Ok(serde_json::to_string(&e)?)
}

//code to analyze bitstamp data
// use tokio::net::TcpStream;
// use tokio_tungstenite::tungstenite::protocol::Message;
// use tokio_tungstenite::connect_async;
// use futures::{StreamExt, SinkExt};

// #[tokio::main]
// async fn main() {
//     // Connect to Bitstamp's WebSocket for order_book channel.
//     let url = "wss://ws.bitstamp.net";
//     let (mut ws_stream, _) = connect_async(url).await.expect("Error connecting");

//     println!("Connected to Bitstamp's order_book WebSocket feed!");

//     let (mut write, mut read) = ws_stream.split();

//     // Subscribe to the order_book channel for a specific pair, e.g., btcusd.
//     let subscribe_msg = Message::Text(r#"{
//         "event": "bts:subscribe",
//         "data": {
//             "channel": "order_book_ethbtc"
//         }
//     }"#.to_string());

//     write.send(subscribe_msg).await.expect("Failed to send subscribe message");

//     // ws_stream.send(subscribe_msg).await.expect("Failed to send subscribe message");

//     // Now, split the stream.
//     // let (write, mut read) = ws_stream.split();

//     // You can continue to use `write` if you want to send more messages later.
//     // However, for now, we just read the incoming messages.
//     while let Some(message) = read.next().await {
//         match message {
//             Ok(Message::Text(text)) => {
//                 println!("Received Text: {}", text);
//             }
//             Ok(Message::Close(_)) => {
//                 println!("Connection closed by server");
//                 break;
//             }
//             Err(e) => {
//                 println!("Error: {:?}", e);
//             }
//             _ => {}
//         }
//     }
// }


#[cfg(test)]
mod test {
    use rust_decimal_macros::dec;
    use crate::bitstamp::*;

    #[test]
    fn test_deserialize() -> Result<(), Error> {
        // assert_eq!(deserialize(r#"{"data":{"timestamp":"1693788732","microtimestamp":"1693788732161670",
        // "bids":[["0.06310274","0.70268645"],["0.06310273","0.39939066"],
        // ["0.06310269","6.68454406"],["0.06309940","0.50000000"]],
        // "asks":[["0.06314222","0.91722040"],["0.06314509","0.50000000"],
        // ["0.06315963","0.50000000"],["0.06316000","0.03764527"]],
        // "channel":"order_book_ethbtc","event":"data"
        //            }"#.to_string())?,
                   assert_eq!(deserialize("{\
                    \"data\":{\
                        \"timestamp\":\"1693788732\",\
                        \"microtimestamp\":\"1693788732161670\",\
                        \"bids\":[[\"0.06310274\",\"0.70268645\"],[\"0.06310273\",\"0.39939066\"],\
                        [\"0.06310269\",\"6.68454406\"],[\"0.06309940\",\"0.50000000\"]],\
                        \"asks\":[[\"0.06314222\",\"0.91722040\"],[\"0.06314509\",\"0.50000000\"],\
                        [\"0.06315963\",\"0.50000000\"],[\"0.06316000\",\"0.03764527\"]]\
                    },\
                    \"channel\":\"order_book_ethbtc\",\
                    \"event\":\"data\"\
                }".to_string())?,
                   Event::Data{
                       data: InData {

                           bids: vec![
                               Level { price: dec!(0.06310274), amount: dec!(0.70268645) },
                               Level { price: dec!(0.06310273), amount: dec!(0.39939066) },
                               Level { price: dec!(0.06310269), amount: dec!(6.68454406) },
                               Level { price: dec!(0.06309940), amount: dec!(0.50000000) },
                           ],
                           asks: vec![
                               Level { price: dec!(0.06314222), amount: dec!(0.91722040) },
                               Level { price: dec!(0.06314509), amount: dec!(0.50000000) },
                               Level { price: dec!(0.06315963), amount: dec!(0.50000000) },
                               Level { price: dec!(0.06316000), amount: dec!(0.03764527) },
                           ],
                       },
                       channel: "order_book_ethbtc".to_string(),
                   });
        Ok(())
    }
}

