use crate::error_handling::{Error, ExchangeErr};
use crate::grpc_server::OrderBookService;
use crate::order_collection::{Exchanges, InputData, OutputData};
use crate::{bitstamp,binance, websocket};
// use futures::channel::mpsc::UnboundedSender;
// use tokio::sync::mpsc::{UnboundedReceiver,UnboundedSender}; //we are using UnboundedReceiverStream instead
use tokio::sync::mpsc::UnboundedSender;
use log::{debug, error, info};
use std::sync::Arc;
// use tokio::sync::{RwLock, watch};
use tokio::sync::watch;
use tungstenite::protocol::Message;
use futures::{SinkExt, StreamExt};
use futures::join;
use tokio_stream::wrappers::UnboundedReceiverStream;
// use tokio::stream::{StreamExt, SinkExt};  //this says consider importing from futures
// use tokio::join;
use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc;
//tokio::sync::mpsc::UnboundedReceiver does not have the stream trait implemented for it in the more recent versions of tokio.
// So I used the UnboundedReceiverStream from tokio_stream which has the stream trait implemented. UnboundedReceiverStream wraps
//UnboundedReceiver. The stream trait belongs to the futures crate and helps with using methods like next() on streams. We could have
// also used UnboundedReceiver from futures.


//The rx function is used for sending user inputs and receiving messages via a bounded mpsc channel.
//It's continuously listening for user messages within a loop. There is buffer reader for reading user inputs. If the user sends
// "/exit", the loop breaks and the program exits.
pub fn rx() -> Receiver<String> {
    let (tx_stdin, rx_stdin) = mpsc::channel::<String>(10);
    // read from stdin. async means asynchronous block, move means that the block can take ownership of values outside of it.
    let stdin_loop = async move {
        loop {
            let mut buf_stdin = tokio::io::BufReader::new(tokio::io::stdin());
            let mut line = String::new();
            buf_stdin.read_line(&mut line).await.unwrap();
            tx_stdin.send(line.trim().to_string()).await.unwrap();
            if line.trim() == "/exit" {
                break;
            }
        }
    };
    //spawn is used to run the loop in the background while the program continues. 
    tokio::task::spawn(stdin_loop);
    rx_stdin
}

pub async fn run(
    currency_pair: &String,
    port: usize,
    no_bitstamp: bool,
    no_binance: bool,
) -> Result<(), Error>
{
    let connector = Connector::new();
    let service = OrderBookService::new(connector.out_ticks.clone());
    // let service = OrderBookService::new((connector.out_ticks.0.c, connector.out_ticks.1));

    tokio::spawn(async move {
        service.serve(port).await.expect("Failed to serve grpc");
    });

    connector.run(currency_pair,
                  no_bitstamp, no_binance).await?;

    Ok(())
}

//a watch channel is created through which OutputData can be broadcasted on multiple threads or a single thread from a single thread.
//broadcasting is done on different threads by cloning the receiving end of the channel. Cloning in case of channels means 
//to create references to the receiving end of the channel (unlike other dtypes where it means a deep copy on heap). For mnpc
//channels, the several references (through cloning) can be created for sender side of the channel and for watch, several references of the 
//receiving side (through cloning) can be created.
pub type OutTickPair = (watch::Sender<OutputData>, watch::Receiver<OutputData>);


// The Connector struct had a field of the type Arc<OutTickPair>. Arc (atomic reference counter) is used around the watch channel because
// one can create multiple references to the receiver side (multiple threads referring to the receiver side) of the watch channel using .clone()
// and Arc provides a thread safe way of doing that. 
struct Connector {
    // out_ticks: Arc<RwLock<OutTickPair>>,
    out_ticks: Arc<OutTickPair>,
}


impl Connector {
    fn new() -> Connector {
        // let out_ticks = Arc::new(RwLock::new(watch::channel(OutTick::new())));
        // Connector { out_ticks }
        let out_ticks = Arc::new(watch::channel(OutputData::new()));
        Connector { out_ticks }
    }

    async fn run(
        &self,
        currency_pair: &String,
        skip_bitstamp: bool,
        skip_binance: bool,

    ) -> Result<(), Error>
    {
        //putting the async connections to binance and bitstamp under a join! so that they both run independently and we get a 
        // tuple Ready when both the tasks are Completed.
        let (
            ws_bitstamp,
            ws_binance,
        ) = join!(
            bitstamp::bitstamp_connect(currency_pair),
            binance::binance_connect(currency_pair),

        );
        let mut ws_bitstamp = ws_bitstamp?;
        let mut ws_binance = ws_binance?;


        let mut rx_stdin = rx();
        // let (tx_in_ticks, mut rx_in_ticks) = futures::channel::mpsc::unbounded();
        let (tx_in_ticks, rx_in_ticks) = mpsc::unbounded_channel();
        let mut rx_in_ticks = UnboundedReceiverStream::new(rx_in_ticks);
        let mut exchanges = Exchanges::new();

        // handle websocket messages. select! is used so that whichever task within the block is complete first, the branch associated
        //to that is executed first, and this continues continuously since it is within a loop (unless an error occurs and the loop breaks)
        loop {
            tokio::select! {

                ws_msg = ws_bitstamp.next() => {
                    // we clone (different from deep copy) or create a reference to the sending side of the mpsc channel so that a thread is responsible for
                    //sending only the bitstamp data to the receiving end. On this sender side of the channel, some parsing is
                    // done after which it is sent to a single receive side (single because that cannot be refered to or cloned).
                    let tx = tx_in_ticks.clone();

                    let res = handle(ws_msg)
                        .and_then(|msg| {
                            if skip_bitstamp { Ok(()) }
                            else { msg.parse_and_send(bitstamp::parse, tx) }
                        })
                        .map_err(ExchangeErr::Bitstamp);

                    if let Err(e) = res {
                        error!("Err: {:?}", e);
                        break
                    }
                },
                ws_msg = ws_binance.next() => {
                    // we create another clone (different from deep copy) or create another  reference to the sending side of the mpsc channel so that a different thread is responsible for
                    //sending only the binance data to the receiving end of the mpsc channel. 
                    let tx = tx_in_ticks.clone();

                    let res = handle(ws_msg)
                        .and_then(|msg| {
                            if skip_binance { Ok(()) }
                            else { msg.parse_and_send(binance::parse, tx) }
                        })
                        .map_err(ExchangeErr::Binance);

                    if let Err(e) = res {
                        error!("Err: {:?}", e);
                        break
                    }
                },

                //await the receival of input message on the receive side of the mpsc used for the user inputs
                stdin_msg = rx_stdin.recv() => {
                    match stdin_msg {
                        Some(msg) => {
                            info!("Sent to WS: {:?}", msg);
                            let _ = ws_binance.send(Message::Text(msg)).await;
                        },
                        None => break,
                    }
                },

                //once messages from binance or bitstamp are received on the receiving end of mpsc, they are combined and processed 
                // to get top 10 asks, bids and spread (OutputData). These are then sent via the sending side of the watch channel we
                //created.
                in_tick = rx_in_ticks.next() => {
                    match in_tick {
                        Some(t) => {
                            debug!("{:?}", t);
                            exchanges.update(t);

                            let out_tick = exchanges.to_tick();
                            debug!("{:?}", out_tick);

                            // let writer = self.out_ticks.write().await;
                            // let tx = &writer.0;
                            let tx = &self.out_ticks.0;
                            tx.send(out_tick).expect("channel should not be closed");
                            // tx.send(out_tick).expect("channel should not be closed");
                        },
                        _ => {},
                    }
                },
            };
        }

        // Gracefully close connection by Close-handshake procedure
        join!(
            websocket::close(&mut ws_bitstamp),
            websocket::close(&mut ws_binance),

        );

        Ok(())
    }
}

fn handle(
    ws_msg: Option<Result<Message, tungstenite::Error>>,
) -> Result<Message, Error>
{
    // let msg = match ws_msg {
    //     Some(inner_msg) => inner_msg,
    //     None => {
    //         info!("no message");
    //         Err(tungstenite::Error::ConnectionClosed)
    //     }
    // }?;  //the ? will convert tungstenite::Error::ConnectionClosed into our custom Error type.
    let msg = ws_msg.unwrap_or_else(|| {
        info!("no message");
        Err(tungstenite::Error::ConnectionClosed)
    })?;

    Ok(msg)
}

trait ParseAndSend {
    fn parse_and_send(
        self,
        parse: fn(Message) -> Result<Option<InputData>, Error>,
        tx: UnboundedSender<InputData>,
    ) -> Result<(), Error>;
}

impl ParseAndSend for Message {
    fn parse_and_send(
        self,
        parse: fn(Message) -> Result<Option<InputData>, Error>,
        tx: UnboundedSender<InputData>,
    ) -> Result<(), Error>
    {
        // match parse(self) {
        //     Ok(t) => {
        //         if let Some(tick) = t {
        //             tokio::spawn(async move {                
        //                 tx.send(tick).expect("Failed to send");
        //             });
        //         }
        //         Ok(())
        //     },
        //     Err(e) => Err(e),
        // }
        parse(self).and_then(|t| {
            t.map(|tick| {
                tokio::spawn(async move {
                    // tx.unbounded_send(tick).expect("Failed to send");
                    tx.send(tick).expect("Failed to send");
                });
            });
            Ok(())
        })
    }
}



// The difference between async/await and spawn
    // is that await will pause the task and let other programs continue on the thread till the future is ready, whereas spawn
    //will let the other parts of the program continue while the spawned task runs or pauses in the background.

    //I had earlier wrapped the watch channel inside a RwLock. I think the purpose of that was to avoid accessing data using
    // .borrow() method on the receiving side (that creates a reference to the latest value in the receiving side) at the same time
    // as send is being used on the sender side of the channel. With Rwlock one can create several read locks on the
    // receiving side (several threads accessing the receiving side) and a single write lock (a single thread) on the sending side. 

    //In the context of RwLock, I also ended up learning about other concepts like RefCell, interior mutability and dynamic borrowing.
    //Though not designed for that purpose, but RwLock provides interior mutability and dynamic borrowing. On refcell, borrow() and
    // borrow_mut() are used for dynammic borrowing. This borrow() is completely different from the borrow() used on receiving end
    //of watch channel. 