//! Loopback TCP relay for the live tests and `reconnect_watch`.
//!
//! Bytes are copied. They are not parsed and not logged. The ticket stays
//! inside the upgrade request.

#![allow(dead_code)]

use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Forward,
    Blackhole,
    Refuse,
}

enum Command {
    Drop,
    Blackhole,
    Refuse,
    Restore,
    #[cfg(test)]
    Barrier(tokio::sync::oneshot::Sender<()>),
}

struct Open {
    abort: tokio::task::AbortHandle,
    parked: bool,
}

/// A relay bound to `127.0.0.1:0` that forwards to the gateway's WebSocket port.
pub struct FaultRelay {
    origin: String,
    port: u16,
    commands: mpsc::UnboundedSender<Command>,
}

impl FaultRelay {
    /// Bind and forward new connections to `target` until a fault is requested.
    pub async fn start(target: SocketAddr) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let (commands, inbox) = mpsc::unbounded_channel();
        tokio::spawn(run(listener, target, inbox));
        Ok(Self {
            origin: format!("http://127.0.0.1:{port}"),
            port,
            commands,
        })
    }

    /// `http://127.0.0.1:<port>`, suitable for [`crate::liveness::GatewayDial::via_origin`].
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// The loopback port this relay accepts on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Close both sides of every open connection. No WebSocket close frame is sent.
    pub fn drop(&self) {
        let _ = self.commands.send(Command::Drop);
    }

    /// Stop forwarding. Open connections stay open. New connections are held.
    pub fn blackhole(&self) {
        let _ = self.commands.send(Command::Blackhole);
    }

    /// Accept each new connection and close it at once.
    pub fn refuse(&self) {
        let _ = self.commands.send(Command::Refuse);
    }

    /// Close connections held since [`blackhole`](Self::blackhole), then forward again.
    pub fn restore(&self) {
        let _ = self.commands.send(Command::Restore);
    }
}

async fn run(
    listener: TcpListener,
    target: SocketAddr,
    mut inbox: mpsc::UnboundedReceiver<Command>,
) {
    let mut mode = Mode::Forward;
    let (paused_tx, paused_rx) = watch::channel(false);
    let mut open: Vec<Open> = Vec::new();
    let mut held: Vec<TcpStream> = Vec::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { break };
                match mode {
                    Mode::Refuse => drop(stream),
                    Mode::Blackhole => held.push(stream),
                    Mode::Forward => {
                        let handle = tokio::spawn(forward(stream, target, paused_rx.clone()));
                        open.push(Open {
                            abort: handle.abort_handle(),
                            parked: false,
                        });
                    }
                }
            }
            command = inbox.recv() => {
                let Some(command) = command else { break };
                match command {
                    Command::Drop => {
                        for item in open.drain(..) {
                            item.abort.abort();
                        }
                        held.clear();
                    }
                    Command::Blackhole => {
                        mode = Mode::Blackhole;
                        let _ = paused_tx.send(true);
                        for item in &mut open {
                            item.parked = true;
                        }
                    }
                    Command::Refuse => {
                        mode = Mode::Refuse;
                    }
                    Command::Restore => {
                        open.retain(|item| {
                            if item.parked {
                                item.abort.abort();
                                false
                            } else {
                                true
                            }
                        });
                        held.clear();
                        mode = Mode::Forward;
                        let _ = paused_tx.send(false);
                    }
                    #[cfg(test)]
                    Command::Barrier(ready) => { let _ = ready.send(()); }
                }
            }
        }
    }
}

async fn forward(mut client: TcpStream, target: SocketAddr, mut paused: watch::Receiver<bool>) {
    let Ok(mut upstream) = TcpStream::connect(target).await else {
        return;
    };
    let mut client_buf = [0u8; 4096];
    let mut upstream_buf = [0u8; 4096];
    loop {
        if *paused.borrow() {
            if paused.changed().await.is_err() {
                return;
            }
            continue;
        }
        tokio::select! {
            biased;
            changed = paused.changed() => {
                if changed.is_err() {
                    return;
                }
            }
            read = client.read(&mut client_buf) => {
                if copy_chunk(read, &mut upstream, &client_buf).await.is_err() {
                    return;
                }
            }
            read = upstream.read(&mut upstream_buf) => {
                if copy_chunk(read, &mut client, &upstream_buf).await.is_err() {
                    return;
                }
            }
        }
    }
}

async fn copy_chunk(
    read: std::io::Result<usize>,
    writer: &mut TcpStream,
    buf: &[u8],
) -> std::io::Result<()> {
    let n = read?;
    if n == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "closed",
        ));
    }
    writer.write_all(&buf[..n]).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const DEADLINE: Duration = Duration::from_secs(2);

    async fn barrier(relay: &FaultRelay) {
        let (ready, received) = tokio::sync::oneshot::channel();
        relay.commands.send(Command::Barrier(ready)).unwrap();
        tokio::time::timeout(DEADLINE, received)
            .await
            .unwrap()
            .unwrap();
    }

    async fn fixture() -> (FaultRelay, tokio::task::JoinHandle<()>) {
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay = FaultRelay::start(target.local_addr().unwrap())
            .await
            .unwrap();
        let echo = tokio::spawn(async move {
            let mut peers = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = target.accept() => {
                        let (mut stream, _) = accepted.unwrap();
                        peers.spawn(async move {
                            let (mut reader, mut writer) = stream.split();
                            let _ = tokio::io::copy(&mut reader, &mut writer).await;
                        });
                    }
                    Some(_) = peers.join_next(), if !peers.is_empty() => {}
                }
            }
        });
        (relay, echo)
    }

    async fn connect(relay: &FaultRelay) -> TcpStream {
        tokio::time::timeout(DEADLINE, TcpStream::connect(("127.0.0.1", relay.port())))
            .await
            .unwrap()
            .unwrap()
    }

    async fn echo(stream: &mut TcpStream, bytes: &[u8]) {
        tokio::time::timeout(DEADLINE, async {
            stream.write_all(bytes).await.unwrap();
            let mut received = vec![0; bytes.len()];
            stream.read_exact(&mut received).await.unwrap();
            assert_eq!(received, bytes);
        })
        .await
        .expect("relay forwards both directions");
    }

    async fn closed(stream: &mut TcpStream) {
        let mut byte = [0];
        let read = tokio::time::timeout(DEADLINE, stream.read(&mut byte))
            .await
            .expect("relay closes the socket");
        assert!(
            matches!(read, Ok(0)) || read.is_err(),
            "closed socket forwarded bytes"
        );
    }

    async fn held_open_without_forwarding(stream: &mut TcpStream) {
        stream.write_all(b"held").await.unwrap();
        let mut byte = [0];
        assert!(
            tokio::time::timeout(Duration::from_millis(50), stream.read(&mut byte))
                .await
                .is_err(),
            "blackholed socket must stay open without forwarding"
        );
    }

    #[tokio::test]
    async fn drop_closes_existing_connections_then_new_traffic_forwards() {
        let (relay, target) = fixture().await;
        let mut first = connect(&relay).await;
        let mut second = connect(&relay).await;
        echo(&mut first, b"one").await;
        echo(&mut second, b"two").await;
        relay.drop();
        barrier(&relay).await;
        closed(&mut first).await;
        closed(&mut second).await;
        let mut fresh = connect(&relay).await;
        echo(&mut fresh, b"fresh").await;
        target.abort();
    }

    #[tokio::test]
    async fn blackhole_holds_old_and_new_connections_until_restore_closes_them() {
        let (relay, target) = fixture().await;
        let mut existing = connect(&relay).await;
        echo(&mut existing, b"before").await;
        relay.blackhole();
        barrier(&relay).await;
        let mut held = connect(&relay).await;
        held_open_without_forwarding(&mut existing).await;
        held_open_without_forwarding(&mut held).await;
        relay.restore();
        barrier(&relay).await;
        closed(&mut existing).await;
        closed(&mut held).await;
        let mut fresh = connect(&relay).await;
        echo(&mut fresh, b"restored").await;
        target.abort();
    }

    #[tokio::test]
    async fn refuse_closes_new_connections_while_existing_traffic_keeps_forwarding() {
        let (relay, target) = fixture().await;
        let mut existing = connect(&relay).await;
        echo(&mut existing, b"before").await;
        relay.refuse();
        barrier(&relay).await;
        let mut refused = connect(&relay).await;
        closed(&mut refused).await;
        echo(&mut existing, b"during refusal").await;
        relay.restore();
        barrier(&relay).await;
        echo(&mut existing, b"after restore").await;
        let mut fresh = connect(&relay).await;
        echo(&mut fresh, b"new").await;
        target.abort();
    }
}
