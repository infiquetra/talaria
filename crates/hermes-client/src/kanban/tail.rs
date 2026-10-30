use super::{KanbanError, TailBatch};
use crate::rest::{RestClient, RestOptions, TicketMinter};
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::{
    net::TcpStream,
    sync::mpsc,
    task::JoinHandle,
    time::{Instant, sleep, sleep_until},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
pub struct KanbanTail {
    receiver: mpsc::UnboundedReceiver<Result<TailBatch, KanbanError>>,
    task: JoinHandle<()>,
}
impl KanbanTail {
    pub async fn connect(
        origin: &str,
        board: &str,
        since: i64,
        minter: Arc<dyn TicketMinter>,
    ) -> Result<Self, KanbanError> {
        // Validate the literal loopback authority using the shared transport's policy.
        let _ = RestClient::new(origin, minter.clone(), RestOptions::default())?;
        let socket = open(origin, board, since, &minter).await?;
        let (sender, receiver) = mpsc::unbounded_channel();
        let (origin, board) = (origin.to_owned(), board.to_owned());
        let task = tokio::spawn(async move {
            let (mut socket, mut cursor) = (socket, since);
            let mut backoff = 0_u32;
            loop {
                let mut idle = Instant::now() + Duration::from_secs(30);
                loop {
                    tokio::select! {
                        _ = sender.closed() => return,
                        _ = sleep_until(idle) => {
                            if socket.send(Message::Text("{\"type\":\"ping\"}".into())).await.is_err() { break; }
                            idle = Instant::now() + Duration::from_secs(30);
                        }
                        frame = socket.next() => {
                            idle = Instant::now() + Duration::from_secs(30);
                            if frame.as_ref().is_some_and(Result::is_ok) {backoff=0;}
                            match frame {
                                Some(Ok(Message::Text(text))) => {
                                    match serde_json::from_str::<TailBatch>(&text) {
                                        Ok(batch) => { cursor = batch.cursor; if sender.send(Ok(batch)).is_err() { return; } }
                                        Err(_) => break,
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                                Some(Ok(_)) => {}
                            }
                        }
                    }
                }
                let mut connected = None;
                for _ in 0..3 {
                    let delay = if backoff < 5 { 1_u64 << backoff } else { 30 };
                    backoff = backoff.saturating_add(1);
                    tokio::select! { _ = sender.closed() => return, _ = sleep(Duration::from_secs(delay)) => {} }
                    if let Ok(next) = open(&origin, &board, cursor, &minter).await {
                        connected = Some(next);
                        break;
                    }
                }
                match connected {
                    Some(next) => socket = next,
                    None => {
                        let _ = sender.send(Err(KanbanError::Message(
                            "live updates unavailable; press r to refresh".into(),
                        )));
                        return;
                    }
                }
            }
        });
        Ok(Self { receiver, task })
    }
    pub async fn recv(&mut self) -> Result<TailBatch, KanbanError> {
        self.receiver.recv().await.unwrap_or_else(|| {
            Err(KanbanError::Message(
                "live updates unavailable; press r to refresh".into(),
            ))
        })
    }
}
impl Drop for KanbanTail {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn open(
    origin: &str,
    board: &str,
    since: i64,
    minter: &Arc<dyn TicketMinter>,
) -> Result<Socket, KanbanError> {
    let mut url = url::Url::parse(&origin.replacen("http://", "ws://", 1))
        .map_err(|_| KanbanError::Message("invalid board origin".into()))?;
    url.set_path("/api/plugins/kanban/events");
    url.query_pairs_mut()
        .append_pair("since", &since.to_string())
        .append_pair("board", board);
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|_| KanbanError::Message("invalid events route".into()))?;
    let ticket = minter.mint().await.map_err(|_| KanbanError::Message("the gateway refused the board request; check the gateway is running and this profile may use it".into()))?;
    request.headers_mut().insert(
        "x-hermes-gateway-ticket",
        ticket
            .expose()
            .parse()
            .map_err(|_| KanbanError::Message("could not authenticate board updates".into()))?,
    );
    tokio::time::timeout(Duration::from_secs(15), connect_async(request))
        .await
        .map_err(|_| KanbanError::Message("board events connection timed out".into()))?
        .map(|(socket, _)| socket)
        .map_err(|_| KanbanError::Message("could not connect board updates".into()))
}
