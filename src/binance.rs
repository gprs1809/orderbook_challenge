use crate::error_handling::Error;
use crate::order_collection::{self, Exchange, InputData, ToLevel, ToLevels, ToTick};
use crate::websocket;
use log::{debug, info};
use rust_decimal::Decimal;
use serde::Deserialize;
use tungstenite::Message;

const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/ws";

//the BinanceDataFormat struct captures the fields in the order details from Binance (like the example used in unit testing below)
//3 traits (debug, deserialize & PartialEq) have been derived for this struct (with their default implementations). Deserialize trait is from the serde crate. Serde crate helps with serializing rust instances of structs into strings or json (serialize) and vice versa (deserialize)
//PartialEq trait implements (in)equality for the binancedataformat type and considers the NaN cases as well. 
//PartialEq is important for assert_eq in testing
//Debug is required to be derived/implemented because we use the debug marco (debug!) and "{:?}" within debug!
#[derive(Debug, Deserialize, PartialEq)]
struct BinanceDataFormat {
    #[serde(rename = "lastUpdateId")]
    last_update_id: usize,
    bids: Vec<Level>,
    asks: Vec<Level>,
}

//Level struct captures the fields within a single order (Price and Amount). These details along with the exchange name form the Level as described in the protobuf file
//We need to derive the Clone trait here because this Level is converted to order_collection::Level and so has to be cloned because order_collection:: Level has to be cloned (in order_collection.rs and in grpc_server.rs)
#[derive(Debug, Deserialize, PartialEq, Clone)]
struct Level {
    price: Decimal,
    amount: Decimal,
}

impl ToLevel for Level {
    /// Converts a binance::Level into a order_collection::Level. 
    fn to_level(&self, side: order_collection::Side) -> order_collection::Level {
        order_collection::Level::new(side, self.price, self.amount, Exchange::Binance)
    }
}

impl ToTick for BinanceDataFormat {
    /// Converts the BinanceDataFormat into a Option<InTick>. Only keep the top ten levels of bids and asks.
    fn option_intick(&self) -> Option<InputData> {
        let bids = self.bids.to_levels(order_collection::Side::Bid, 10);
        let asks = self.asks.to_levels(order_collection::Side::Ask, 10);

        Some(InputData { exchange: Exchange::Binance, bids, asks })
    }
}

//below asynchronously connects to the binance websocket. The general asynchronous connect function is defined in websocket.rs
pub async fn binance_connect(currency_pair: &String) -> Result<websocket::WsStream, Error> {
    let depth = 10;
    let symbol = currency_pair.to_lowercase().replace("/", "");
    let url = format!("{}/{}@depth{}@100ms", BINANCE_WS_URL, symbol, depth); //we will take top 10 from each exchange since 
    //in the combined orderbook, we will be taken top 10 from everything combined.
    Ok(websocket::connect(url.as_str()).await?)
}

//parse function is for parsing message frames from the websocket. 
//Only text messages are useful and they are deserialized into InputData data type
//All other messages like Ping, Pong and other message frames are mapped to None
//Error is a custom error type created in error_handling.rs
//The ? operator on deserialize(x)?  maps any error into the Error type and if no error, it unwraps the result within Ok
pub fn parse(msg: Message) -> Result<Option<InputData>, Error> {
    let e = match msg {
        Message::Binary(x) => { info!("binary {:?}", x); None },

        // let e = deserialize(x)? is equivalent to
        // let e = match derialize(x) {
        //     Ok(v) => v,
        //     Err(e) => return Err(Error),
        // }
        //the {:?} within debug macro can be done because the Debug trait is implemented for BinanceDataFormat
        Message::Text(x) => {
            let e= deserialize(x)?;
            debug!("{:?}", e);
            Some(e)
        },
        Message::Ping(x) => { info!("Ping {:?}", x); None },
        Message::Pong(x) => { info!("Pong {:?}", x); None },
        Message::Close(x) => { info!("Close {:?}", x); None },
        Message::Frame(x) => { info!("Frame {:?}", x); None },
    };
    Ok(e.map(|e| e.option_intick()).flatten())
}

fn deserialize(s: String) -> serde_json::Result<BinanceDataFormat> {
    Ok(serde_json::from_str(&s)?)
}

///code for seeing how binance data looks:
// use tungstenite::connect;
// use url::Url;
// use tungstenite::protocol::Message;

// static BINANCE_WS_API: &str = "wss://stream.binance.com:9443";
// // const BITSTAMP_WS_URL: &str = "wss://ws.bitstamp.net";

// // #[tokio::main]
// fn main() {
//     let binance_url = format!("{}/ws/ethbtc@depth5@100ms", BINANCE_WS_API);
//     let (mut socket, response) =
//         connect(Url::parse(&binance_url).unwrap()).expect("Can't connect.");
    
//     println!("Connected to binance/bitstamp stream.");
//     println!("HTTP status code: {}", response.status());
//     println!("Response headers:");
//     for (ref header, ref header_value) in response.headers() {
//         println!("- {}: {:?}", header, header_value);
//     }
//     loop {
//         let msg = socket.read_message().expect("Error reading message");
//         let msg = match msg {
//             tungstenite::Message::Text(s) => s,
//             _ => {
//                 panic!("Error getting text");
//             }
//         };
//         println!("{}",msg);
//     }
// }


#[cfg(test)]
mod test {
    use rust_decimal_macros::dec;
    use crate::binance::*;

    #[test]
    fn test_binance_deserialize() -> Result<(), Error> {
        assert_eq!(deserialize(r#"
        {
           "lastUpdateId":6812617950,
           "bids":[["0.06319000","25.87160000"],["0.06318000","20.96280000"],
           ["0.06317000","23.73310000"],["0.06316000","19.45870000"]],
           "asks":[["0.06320000","67.45490000"],["0.06321000","45.78680000"],
           ["0.06322000","23.11950000"],["0.06323000","55.13140000"]]
        }"#.to_string())?,
                   BinanceDataFormat{
                       last_update_id: 6812617950,
                       bids: vec![
                           Level { price: dec!(0.06319000), amount: dec!(25.87160000) },
                           Level { price: dec!(0.06318000), amount: dec!(20.96280000) },
                           Level { price: dec!(0.06317000), amount: dec!(23.73310000) },
                           Level { price: dec!(0.06316000), amount: dec!(19.45870000) },
                       ],
                       asks: vec![
                           Level { price: dec!(0.06320000), amount: dec!(67.45490000) },
                           Level { price: dec!(0.06321000), amount: dec!(45.78680000) },
                           Level { price: dec!(0.06322000), amount: dec!(23.11950000) },
                           Level { price: dec!(0.06323000), amount: dec!(55.13140000) },

                       ]
                   }
        );
        Ok(())
    }
}