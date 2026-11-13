//! A test-only barrier after a fake command completes its side effects.
#![allow(dead_code)]

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::{Duration, Instant};

pub struct Readiness {
    listener: Option<TcpListener>,
    address: SocketAddr,
    point: &'static str,
    peer: Option<TcpStream>,
}

impl Readiness {
    pub fn new(point: &'static str) -> Self {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind readiness listener");
        listener
            .set_nonblocking(true)
            .expect("set readiness listener nonblocking");
        let address = listener.local_addr().expect("read readiness address");
        Self {
            listener: Some(listener),
            address,
            point,
            peer: None,
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn point(&self) -> &str {
        self.point
    }

    pub fn wait(&mut self, timeout: Duration, mut ended: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + timeout;
        loop {
            let remaining = end.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            match self
                .listener
                .as_ref()
                .expect("armed readiness listener")
                .accept()
            {
                Ok((mut stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .expect("set readiness stream blocking");
                    stream
                        .set_read_timeout(Some(remaining.min(Duration::from_secs(1))))
                        .expect("set readiness receive timeout");
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .expect("set readiness reply timeout");
                    let expected = format!("talaria-smoke-ready:{}", self.point);
                    let mut message = vec![0; expected.len()];
                    if stream.read_exact(&mut message).is_err() || message != expected.as_bytes() {
                        continue;
                    }
                    self.peer = Some(stream);
                    self.listener.take();
                    return true;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if ended() {
                        return false;
                    }
                    std::thread::sleep(remaining.min(Duration::from_millis(10)));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => panic!("receive fake readiness event: {error}"),
            }
        }
    }

    pub fn release(&mut self) {
        self.listener.take();
        if let Some(mut peer) = self.peer.take() {
            let _ = peer.write_all(b"1");
        }
    }
}

impl Drop for Readiness {
    fn drop(&mut self) {
        self.release();
    }
}

pub fn notify(point: &str) -> io::Result<()> {
    if std::env::var("SMOKE_READY_POINT").as_deref() != Ok(point) {
        return Ok(());
    }
    let Some(address) = std::env::var("SMOKE_READY_ADDRESS").ok() else {
        return Ok(());
    };
    let address: SocketAddr = address
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut stream = match TcpStream::connect_timeout(&address, Duration::from_secs(1)) {
        Ok(stream) => stream,
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => return Ok(()),
        Err(error) => return Err(error),
    };
    stream.set_read_timeout(Some(Duration::from_secs(240)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(format!("talaria-smoke-ready:{point}").as_bytes())?;
    let mut reply = [0];
    match stream.read_exact(&mut reply) {
        Ok(()) if reply == *b"1" => Ok(()),
        Ok(()) => Err(io::Error::other("invalid readiness acknowledgement")),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}
