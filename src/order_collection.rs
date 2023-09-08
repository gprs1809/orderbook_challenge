use std::cmp::Ordering;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

//PartialEq trait is implements (in)equality for the InputData type and considers the NaN cases as well. 
//PartialEq is important for assert_eq in testing
//Debug is required to be derived/implemented because we use the debug marco (debug!) and "{:?}" within debug!
#[derive(Debug, PartialEq)]
pub struct InputData {
    pub exchange: Exchange,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

pub trait ToTick {
    fn option_intick(&self) -> Option<InputData>;
}

//The OutputData struct has fields similar to InputData, but spread is a part of this struct.
//Our final outcome that will be published to grpc clients needs to have spread, asks and bids (top 10) and the exchange name.
//We need to derive the Clone trait here because in grpc_servers, 
//the watch channel passes data of the type OutputData within it. In book_summary method inside a trait, we are referencing
//the latest OutputData from the receiving end (using .borrow()) and then cloning it to get ownership or the value itself.
#[derive(Debug, PartialEq, Clone)]
pub struct OutputData {
    pub spread: Decimal,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

impl OutputData {
    pub fn new() -> OutputData {
        OutputData {
            spread: Default::default(),
            bids: vec![],
            asks: vec![],
        }
    }
}

//Eq, PartialEq are required because Exchange type is 1 of the field types in Level for which Ord and PartialOrd are implemented.
//Ord, PartialOrd are derived because in the implementation of these traits for Level, the Exchange type (type for 1 of the fields) is not considered.
//Clone is derived since clone is derived for Level (the reason is given in the comments for Level struct)
#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum Exchange {
    Bitstamp,
    Binance,

}

impl ToString for Exchange {
    fn to_string(&self) -> String {
        match self {
            Exchange::Bitstamp => "bitstamp".to_string(),
            Exchange::Binance => "binance".to_string(),

        }
    }
}

//The reason to derive clone trait is given in detail at the end of the module, just before testing.
//Debug is trait is to be derived so that debug macro (debug!) can be used and "{:?}" can be used.
//Eq and PartialEq are required to be derived for implementing the Ord and PartialOrd traits.
//PartialEq is also required in testing where we use assert_eq!
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Level {
    pub side: Side,
    pub price: Decimal,
    pub amount: Decimal,
    pub exchange: Exchange,
}

impl Level {
    pub fn new(side: Side, price: Decimal, amount: Decimal, exchange: Exchange) -> Level {
        Level{side, price, amount, exchange}
    }
}

// self.price.cmp(&other.price) can be either Ordering::Greater, Ordering::Less or Ordering::Equal 
// eg: if self.price is  30 and other.price is 40, self.price.cmp(&other.price) = Ordering::Less   
// if it is Ordering::Equal and the Side is Bid, then amount is compared.
// by default, if self.amount = 30 and other.amount=40, then self.amount.cmp(&other.amount)=Ordering::Less (30 comes before 40)
// but self.amount.cmp(&other.amount).reverse() will set it to Ordering::Greater which means 40 will come before 30. 
//This ord and then the partialord implementation sorts levels in ascending order w.r.t price. For equal prices,
// levels are sorted w.r.t amounts: ascending order for bids and in descending order for asks. We ultimately want bids to 
//be arranged from highest price to lowest price and asks to be from lowest price to highest price.
impl Ord for Level {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.price.cmp(&other.price), &self.side) {
            (Ordering::Equal, Side::Bid) => self.amount.cmp(&other.amount),
            (Ordering::Equal, Side::Ask) => self.amount.cmp(&other.amount).reverse(),
            (ord, _) => ord,
        }
    }
}

//Similar to the implementation of Ord for Level, but this results in Option<ordering> with None as the result if one of the values being compared is NaN.
impl PartialOrd for Level {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self.price.partial_cmp(&other.price), &self.side) {
            (Some(Ordering::Equal), Side::Bid) => self.amount.partial_cmp(&other.amount),
            (Some(Ordering::Equal), Side::Ask) => self.amount.partial_cmp(&other.amount).map(Ordering::reverse),
            (ord, _) => ord,
        }
    }
}

//Eq, PartialEq are required because Side type is 1 of the field types in Level for which Ord and PartialOrd are implemented.
//Ord, PartialOrd are derived because in the implementation of these traits for Level, the Side type (type for 1 of the fields) is not considered.
//Clone is derived since clone is derived for Level (the reason is given in the comments for Level struct)
#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum Side {
    Bid,
    Ask,
}

// This trait is used to convert Level types from Binance or Bitstamp into Level type defined in this module (order_collection.rs)
pub trait ToLevel {
    fn to_level(&self, side: Side) -> Level;
}

pub trait ToLevels {
    fn to_levels(&self, side: Side, depth: usize) -> Vec<Level>;
}

// For any vec<T>, convert it to vec<order_collection::Level> which is only possible if ToLevel is implemented for type T
// In Binance and Bitstamp, ToLevel is implemented on Binance::Level and Bitstamp::Level
// The ToLevels trait implementation also truncates vec<Level> to have only depth=10 Levels.
// the T type also has clone trait implemented for it because we don't want to take ownership of the original T object, but we also want to return a copy of it with ownership.
// This also one of the reasons why Clone trait was derived for order_collection::Level and also for Side Enum. 
impl<T> ToLevels for Vec<T>
    where T: ToLevel + Clone
{
    fn to_levels(&self, side: Side, depth: usize) -> Vec<Level> {
        let levels = match self.len() > depth {
            true => self.split_at(depth).0.to_vec(), // only keep 10
            false => self.clone(),
        };

        levels.into_iter()
            .map(|l| l.to_level(side.clone()))
            .collect()
    }
}

trait Merge {
    fn merge(self, other: Vec<Level>) -> Vec<Level>;
}

//Merging 2 vectors of type vec<Level>. into_iter() on a vec converts it into an iterator by talking ownership of values in it.
//This is unlike .iter() where it turns vec into an iterator of references of contents in the vec. chain() combines all contents and
// .collect forms a container like vector (inferred by the return type which is vec<Level> here). sort_unstable() works because we 
// implemented Ord and PartialOrd traits on order_collection::Level. The unstable version also moves equal values, but it is 
// considered faster than the stable version.
impl Merge for Vec<Level> {
    fn merge(self, other: Vec<Level>) -> Vec<Level> {
        let mut levels: Vec<Level> =
            self.into_iter()
                .chain(other)
                .collect();
        levels.sort_unstable();
        levels
    }
}

#[derive(Debug, PartialEq)]
struct OrderDepths {
    bids: Vec<Level>,
    asks: Vec<Level>,
}

impl OrderDepths {
    fn new() -> Self {
        OrderDepths {
            bids: vec![],
            asks: vec![],
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Exchanges {
    bitstamp: OrderDepths,
    binance: OrderDepths,

}

impl Exchanges {
    pub fn new() -> Exchanges {
        Exchanges {
            bitstamp: OrderDepths::new(),
            binance: OrderDepths::new(),

        }
    }

    /// Extracts the bids and asks from the instance of Inputdata, then adds into its corresponding
    /// orderbook of the exchange.
    pub fn update(&mut self, t: InputData) {
        match t.exchange {
            Exchange::Bitstamp => {
                self.bitstamp.bids = t.bids;
                self.bitstamp.asks = t.asks;
            },
            Exchange::Binance => {
                self.binance.bids = t.bids;
                self.binance.asks = t.asks;
            }
        }
    }

    /// Returns a new OutputData containing the merge bids and asks from both orderbooks.
    /// Here, rev() is applied to bids to reverse the order by price (the ord implementation sorted in ascending order by price for both asks and bids)
    /// .take(10) takes the top 10 out of the completely merged asks and bids from all the exchanges. 
    pub fn to_tick(&self) -> OutputData {
        let bids: Vec<Level> =
            self.bitstamp.bids.clone()
                .merge(self.binance.bids.clone())
                .into_iter().rev().take(10)
                .collect();

        let asks: Vec<Level> =
            self.bitstamp.asks.clone()
                .merge(self.binance.asks.clone())
                .into_iter().take(10)
                .collect();

        let spread = match (bids.first(), asks.first()) {
            (Some(b), Some(a)) => a.price - b.price,
            (_, _) => dec!(0),
        };

        OutputData { spread, bids, asks }
    }
}

//Why is clone implemented for order_collection::Level, Binance::level, Bitstamp::Level, Exchange, Side:
// Regarding order_collection::Level, here are the reasons for deriving clone trait:
// 1. In the to_levels trait implementation, we are returning a clone or a deep copy of the output which is of the type vec<Level> 
// 2. In the to_tick function, we are cloning each vec<Level> and merging the clones.
// 3. In grpc_server.rs, (OutTickPair is actually defined as (watch::Sender<OutputData>, watch::Receiver<OutputData>)), in the book_summary method, we are using borrow() on the receive end to refer to the OutputData that moves within the watch channel and then we clone() so that we get ownership of a cloned version of the instance of OutputData within the channel, but the OutputData has fields of the type vec<Level> and so order_collection::Level has to have clone derived or implemented. 
// 4. Since orderbook_collection::Level has to be cloned, the fields within it like Side and Exchange also need to have clone trait implemented/derived.
// 5. In case of Binance::Level, this Level is converted to order_collection::Level and so has to be cloned because order_collection:: Level has to be cloned

#[cfg(test)]
mod test {
    use crate::order_collection::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_exchange_update() {
        /*
         * Given
         */
        let mut exchanges = Exchanges::new();
        let t = InputData {
            exchange: Exchange::Bitstamp,
            bids: vec![
                Level::new(Side::Bid, dec!(1), dec!(0.1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(2), dec!(0.2), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(3), dec!(0.3), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(4), dec!(0.4), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(5), dec!(0.5), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(6), dec!(0.6), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(7), dec!(0.7), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(8), dec!(0.8), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(9), dec!(0.9), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(10), dec!(1.0), Exchange::Bitstamp),
            ],
            asks: vec![
                Level::new(Side::Ask, dec!(10), dec!(20), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(30), dec!(40), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(50), dec!(60), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(70), dec!(80), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(90), dec!(100), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(110), dec!(120), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(130), dec!(140), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(150), dec!(160), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(170), dec!(180), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(190), dec!(200), Exchange::Bitstamp),
            ],
        };

        /*
         * When
         */
        exchanges.update(t);

        /*
         * Then
         */
        assert_eq!(exchanges, Exchanges {
            bitstamp: OrderDepths {
                bids: vec![
                    Level::new(Side::Bid, dec!(1), dec!(0.1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(2), dec!(0.2), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(3), dec!(0.3), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(4), dec!(0.4), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(5), dec!(0.5), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(6), dec!(0.6), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(7), dec!(0.7), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(8), dec!(0.8), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(9), dec!(0.9), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(10), dec!(1.0), Exchange::Bitstamp),
                ],
                asks: vec![
                    Level::new(Side::Ask, dec!(10), dec!(20), Exchange::Bitstamp),
                    Level::new(Side::Ask, dec!(30), dec!(40), Exchange::Bitstamp),
                    Level::new(Side::Ask, dec!(50), dec!(60), Exchange::Bitstamp),
                    Level::new(Side::Ask, dec!(70), dec!(80), Exchange::Bitstamp),
                    Level::new(Side::Ask, dec!(90), dec!(100), Exchange::Bitstamp),
                    Level::new(Side::Ask, dec!(110), dec!(120), Exchange::Bitstamp),
                    Level::new(Side::Ask, dec!(130), dec!(140), Exchange::Bitstamp),
                    Level::new(Side::Ask, dec!(150), dec!(160), Exchange::Bitstamp),
                    Level::new(Side::Ask, dec!(170), dec!(180), Exchange::Bitstamp),
                    Level::new(Side::Ask, dec!(190), dec!(200), Exchange::Bitstamp),
                ],
            },
            binance: OrderDepths::new(),

        });
    }

    #[test]
    fn test_merge() {
        /*
         * Given
         */
        let mut exchanges = Exchanges::new();
        let t1 = InputData {
            exchange: Exchange::Binance,
            bids: vec![
                Level::new(Side::Bid, dec!(10), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(9), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(8), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(7), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(6), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(5), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(4), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(3), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(2), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(1), dec!(1), Exchange::Bitstamp),
            ],
            asks: vec![
                Level::new(Side::Ask, dec!(11), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(12), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(13), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(14), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(15), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(16), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(17), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(18), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(19), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(20), dec!(1), Exchange::Bitstamp),
            ],
        };
        let t2 = InputData {
            exchange: Exchange::Bitstamp,
            bids: vec![
                Level::new(Side::Bid, dec!(10.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(9.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(8.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(7.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(6.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(5.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(4.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(3.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(2.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(1.5), dec!(2), Exchange::Binance),
            ],
            asks: vec![
                Level::new(Side::Ask, dec!(11.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(12.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(13.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(14.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(15.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(16.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(17.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(18.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(19.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(20.5), dec!(2), Exchange::Binance),
            ],
        };
  
        exchanges.update(t1);
        exchanges.update(t2);


        /*
         * When
         */
        let out_tick = exchanges.to_tick();

        /*
         * Then
         */
        assert_eq!(out_tick, OutputData {
            spread: dec!(0.5),
            bids:vec![
                Level::new(Side::Bid, dec!(10.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(10), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(9.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(9), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(8.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(8), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(7.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(7), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Bid, dec!(6.5), dec!(2), Exchange::Binance),
                Level::new(Side::Bid, dec!(6), dec!(1), Exchange::Bitstamp),
            ],
            asks: vec![
                Level::new(Side::Ask, dec!(11), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(11.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(12), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(12.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(13), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(13.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(14), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(14.5), dec!(2), Exchange::Binance),
                Level::new(Side::Ask, dec!(15), dec!(1), Exchange::Bitstamp),
                Level::new(Side::Ask, dec!(15.5), dec!(2), Exchange::Binance),
            ],
        });
    }
}