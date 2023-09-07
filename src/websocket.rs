use crate::error_handling::Error;
use futures::{SinkExt, StreamExt};
use log::info;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tungstenite::Message;
use url::Url;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn connect(s: &str) -> Result<WsStream, Error> {
    let url = Url::parse(s).unwrap();
    let (ws_stream, _) =
        tokio_tungstenite::connect_async(url).await?;
    info!("Successfully connected to {}", s);
    Ok(ws_stream)
}

//first a Message::Close is send on the ws_stream (Sink trait from futures is implemented for websocket streams). 
//the response is checked on ws_stream using next() (next is a method in the stream trait from futures which is implemented for ws streams).
//further, next message on the stream is evaluated and checked if it is None. If it is, the connection is asynchronously closed.
pub async fn close(ws_stream: &mut WsStream) {
    let _ = ws_stream.send(Message::Close(None)).await;
    let close = ws_stream.next().await;
    info!("server close msg: {:?}", close);
    assert!(ws_stream.next().await.is_none());
    let _ = ws_stream.close(None).await;
}