//! TURN is a virtual datagram socket: ICE/DTLS/SCTP keep their normal UDP
//! semantics while the client-to-relay leg may use UDP, TCP or verified TLS.
//! The allocation owns its original credentials and refresh timers; replacing
//! signaling credentials must not tear down an active allocation.
use crate::runtime::AsyncUdpSocket;
use bytes::BytesMut;
use rtc::ice::url::{ProtoType, SchemeType, Url};
use rtc::sansio::Protocol;
use rtc::shared::{TaggedBytesMut, TransportContext, TransportProtocol};
use rtc::turn::client::{Client, ClientConfig, Event};
use std::{
    collections::VecDeque,
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
    sync::{Mutex, mpsc, oneshot},
};
use tokio_rustls::{TlsConnector, rustls};

const ALLOCATION_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_DATAGRAM: usize = 2000;
const MAX_PENDING: usize = 16;

struct SendRequest {
    bytes: Vec<u8>,
    peer: SocketAddr,
    queued_at: Instant,
}

pub(crate) struct TurnSocket {
    address: SocketAddr,
    send: mpsc::Sender<SendRequest>,
    receive: Mutex<mpsc::Receiver<(Vec<u8>, SocketAddr)>>,
    // Kept alive until close. Dropping the sender wakes the actor, which
    // deallocates the relay; writes and pending permissions are bounded.
    _task: tokio::task::JoinHandle<()>,
}
impl std::fmt::Debug for TurnSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnSocket")
            .field("relay", &self.address)
            .finish_non_exhaustive()
    }
}

fn fail() -> io::Error {
    io::Error::other("TURN relay transport failed")
}

trait StreamIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> StreamIo for T {}
enum Wire {
    Udp(UdpSocket),
    Stream {
        stream: Box<dyn StreamIo>,
        buffered: Vec<u8>,
    },
}
impl Wire {
    async fn connect(url: &Url) -> io::Result<(Self, SocketAddr, SocketAddr)> {
        let mut peers: Vec<_> = tokio::net::lookup_host((url.host.as_str(), url.port))
            .await?
            .collect();
        peers.sort_by_key(|peer| peer.is_ipv6());
        let peer = *peers.first().ok_or_else(fail)?;
        if url.proto == ProtoType::Udp && url.scheme != SchemeType::Turns {
            let socket = UdpSocket::bind(if peer.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })
            .await?;
            socket.connect(peer).await?;
            let local = socket.local_addr()?;
            return Ok((Self::Udp(socket), local, peer));
        }
        let stream = TcpStream::connect(peers.as_slice()).await?;
        let peer = stream.peer_addr()?;
        stream.set_nodelay(true)?;
        let local = stream.local_addr()?;
        let stream: Box<dyn StreamIo> = if url.scheme == SchemeType::Turns {
            let mut roots = rustls::RootCertStore::empty();
            let native = rustls_native_certs::load_native_certs();
            for cert in native.certs {
                let _ = roots.add(cert);
            }
            let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .map_err(|_| fail())?
            .with_root_certificates(roots)
            .with_no_client_auth();
            config.alpn_protocols = vec![b"stun.turn".to_vec()];
            let name =
                rustls::pki_types::ServerName::try_from(url.host.clone()).map_err(|_| fail())?;
            Box::new(
                TlsConnector::from(Arc::new(config))
                    .connect(name, stream)
                    .await
                    .map_err(|_| fail())?,
            )
        } else {
            Box::new(stream)
        };
        Ok((
            Self::Stream {
                stream,
                buffered: Vec::new(),
            },
            local,
            peer,
        ))
    }
    async fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        tokio::time::timeout(ALLOCATION_TIMEOUT, self.write_frame(bytes))
            .await
            .map_err(|_| fail())?
    }
    async fn write_frame(&mut self, bytes: &[u8]) -> io::Result<()> {
        match self {
            Self::Udp(socket) => {
                socket.send(bytes).await?;
            }
            Self::Stream { stream, .. } => {
                stream.write_all(bytes).await?;
                // RFC 8656: ChannelData is padded to a multiple of four on TCP/TLS.
                if bytes.first().is_some_and(|b| b & 0xc0 == 0x40) {
                    let padding = (4 - bytes.len() % 4) % 4;
                    stream.write_all(&[0; 3][..padding]).await?;
                }
            }
        }
        Ok(())
    }
    async fn receive(&mut self) -> io::Result<Vec<u8>> {
        match self {
            Self::Udp(socket) => {
                let mut bytes = vec![0; 65536];
                let n = socket.recv(&mut bytes).await?;
                bytes.truncate(n);
                Ok(bytes)
            }
            Self::Stream { stream, buffered } => loop {
                if buffered.len() >= 4 {
                    let payload = u16::from_be_bytes([buffered[2], buffered[3]]) as usize;
                    let channel_data = buffered[0] & 0xc0 == 0x40;
                    if buffered[0] & 0xc0 != 0 && !channel_data {
                        return Err(fail());
                    }
                    let size = if channel_data {
                        (4 + payload + 3) & !3
                    } else {
                        20 + payload
                    };
                    if size > 65556 {
                        return Err(fail());
                    }
                    if buffered.len() >= size {
                        return Ok(buffered.drain(..size).collect());
                    }
                }
                // read_buf is cancellation safe; a select cancellation cannot lose
                // a partial STUN header or concatenate separate TCP records.
                if stream.read_buf(buffered).await? == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
            },
        }
    }
}

impl TurnSocket {
    pub(crate) async fn connect(
        url: &str,
        username: &str,
        password: &str,
    ) -> io::Result<Arc<Self>> {
        tokio::runtime::Handle::try_current().map_err(|_| fail())?;
        let url = Url::parse_url(url).map_err(|_| fail())?;
        if !matches!(url.scheme, SchemeType::Turn | SchemeType::Turns)
            || username.is_empty()
            || password.is_empty()
        {
            return Err(fail());
        }
        let (wire, local, server) = tokio::time::timeout(ALLOCATION_TIMEOUT, Wire::connect(&url))
            .await
            .map_err(|_| fail())??;
        // Requested relay protocol is UDP, independently of the control leg.
        // rtc-turn currently derives REQUESTED-TRANSPORT from this field.
        let client = Client::new(ClientConfig {
            stun_serv_addr: String::new(),
            turn_serv_addr: server.to_string(),
            local_addr: local,
            transport_protocol: TransportProtocol::UDP,
            username: username.to_owned(),
            password: password.to_owned(),
            realm: String::new(),
            software: "ctox".to_owned(),
            rto_in_ms: 200,
        })
        .map_err(|_| fail())?;
        let (send, requests) = mpsc::channel(MAX_PENDING);
        let (packets, receive) = mpsc::channel(MAX_PENDING);
        let (allocated, allocation) = oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = run(wire, client, local, server, requests, packets, allocated).await;
        });
        let mut socket = Self {
            address: local,
            send,
            receive: Mutex::new(receive),
            _task: task,
        };
        socket.address = tokio::time::timeout(ALLOCATION_TIMEOUT, allocation)
            .await
            .map_err(|_| fail())?
            .map_err(|_| fail())??;
        Ok(Arc::new(socket))
    }
}

async fn run(
    mut wire: Wire,
    mut client: Client,
    local: SocketAddr,
    server: SocketAddr,
    mut requests: mpsc::Receiver<SendRequest>,
    packets: mpsc::Sender<(Vec<u8>, SocketAddr)>,
    allocated: oneshot::Sender<io::Result<SocketAddr>>,
) -> io::Result<()> {
    client.allocate().map_err(|_| fail())?;
    let mut allocated = Some(allocated);
    let mut relay = None;
    let mut pending = VecDeque::<SendRequest>::new();
    loop {
        if requests.is_closed() {
            if let Some(address) = relay {
                client
                    .relay(address)
                    .map_err(|_| fail())?
                    .close()
                    .map_err(|_| fail())?;
                while let Some(transmit) = client.poll_write() {
                    wire.send(&transmit.message).await?;
                }
            }
            return Ok(());
        }
        while let Some(transmit) = client.poll_write() {
            wire.send(&transmit.message).await?;
        }
        while let Some(event) = client.poll_event() {
            match event {
                Event::AllocateResponse(_, address) => {
                    relay = Some(address);
                    if let Some(reply) = allocated.take() {
                        let _ = reply.send(Ok(address));
                    }
                }
                Event::AllocateError(..) => return Err(fail()),
                Event::DataIndicationOrChannelData(_, peer, bytes) => {
                    // Bound receive memory. Dropping a network datagram under load
                    // preserves UDP semantics; SCTP remains the reliable layer.
                    let _ = packets.try_send((bytes.to_vec(), peer));
                }
                _ => {}
            }
        }
        if let Some(address) = relay {
            let mut waiting = VecDeque::new();
            while let Some(request) = pending.pop_front() {
                let mut allocation = client.relay(address).map_err(|_| fail())?;
                allocation
                    .create_permission(request.peer)
                    .map_err(|_| fail())?;
                if allocation.send_to(&request.bytes, request.peer).is_err()
                    && request.queued_at.elapsed() < ALLOCATION_TIMEOUT
                {
                    waiting.push_back(request);
                }
            }
            pending = waiting;
        }
        while let Some(transmit) = client.poll_write() {
            wire.send(&transmit.message).await?;
        }
        let mut timeout = client
            .poll_timeout()
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(60));
        if let Some(oldest) = pending.front() {
            timeout = timeout.min(oldest.queued_at + ALLOCATION_TIMEOUT);
        }
        tokio::select! {
            request = requests.recv(), if pending.len() < MAX_PENDING => {
                let Some(request) = request else { continue; };
                pending.push_back(request);
            }
            bytes = wire.receive() => {
                client.handle_read(TaggedBytesMut { now: Instant::now(), transport: TransportContext {
                    local_addr: local, peer_addr: server, transport_protocol: TransportProtocol::UDP, ecn: None,
                }, message: BytesMut::from(bytes?.as_slice()) }).map_err(|_| fail())?;
            }
            _ = tokio::time::sleep_until(timeout.into()) => { client.handle_timeout(Instant::now()).map_err(|_| fail())?; }
        }
    }
}

impl AsyncUdpSocket for TurnSocket {
    fn send_to<'a>(
        &'a self,
        bytes: &'a [u8],
        peer: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        Box::pin(async move {
            if bytes.len() > MAX_DATAGRAM {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            self.send
                .send(SendRequest {
                    bytes: bytes.to_vec(),
                    peer,
                    queued_at: Instant::now(),
                })
                .await
                .map_err(|_| fail())?;
            // UDP completion means accepted locally, not remote delivery. Do not
            // hold the ICE/DTLS driver behind a TURN permission roundtrip.
            Ok(bytes.len())
        })
    }
    fn recv_from<'a>(
        &'a self,
        bytes: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<(usize, SocketAddr)>> + Send + 'a>> {
        Box::pin(async move {
            let (packet, peer) = self.receive.lock().await.recv().await.ok_or_else(fail)?;
            if packet.len() > bytes.len() {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            bytes[..packet.len()].copy_from_slice(&packet);
            Ok((packet.len(), peer))
        })
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.address)
    }
}
