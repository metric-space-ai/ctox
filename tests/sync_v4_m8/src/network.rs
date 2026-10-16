//! Test-only UDP impairment below the real DTLS/SCTP stack. No shared interfaces.
use std::{
    fmt::Debug,
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use webrtc::runtime::{AsyncUdpSocket, JoinHandle, Runtime};

#[derive(Debug, Default)]
pub struct Counters {
    pub offered: AtomicU64,
    pub dropped: AtomicU64,
    pub sent: AtomicU64,
    pub failed: AtomicU64,
    pub peak_queue: AtomicU64,
}
#[derive(Debug)]
pub struct ImpairedRuntime {
    pub base: Arc<dyn Runtime>,
    pub delay: Duration,
    pub counters: Arc<Counters>,
    pub seed: AtomicU64,
    pub active: Arc<std::sync::atomic::AtomicBool>,
}
#[derive(Debug)]
struct Packet {
    due: tokio::time::Instant,
    body: Vec<u8>,
    target: SocketAddr,
}
#[derive(Debug)]
struct Socket {
    inner: Arc<dyn AsyncUdpSocket>,
    queue: tokio::sync::mpsc::Sender<Packet>,
    counters: Arc<Counters>,
    random: AtomicU64,
    delay: Duration,
    active: Arc<std::sync::atomic::AtomicBool>,
    next: tokio::sync::Mutex<tokio::time::Instant>,
}
impl Runtime for ImpairedRuntime {
    fn spawn(&self, task: Pin<Box<dyn Future<Output = ()> + Send>>) -> JoinHandle {
        self.base.spawn(task)
    }
    fn wrap_udp_socket(&self, socket: std::net::UdpSocket) -> io::Result<Arc<dyn AsyncUdpSocket>> {
        let inner = self.base.wrap_udp_socket(socket)?;
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Packet>(8192);
        let sink = inner.clone();
        let counters = self.counters.clone();
        // The worker owns no Sender; dropping the PC/socket closes and drains it.
        self.base.spawn(Box::pin(async move {
            while let Some(p) = rx.recv().await {
                tokio::time::sleep_until(p.due).await;
                if sink.send_to(&p.body, p.target).await.is_ok() {
                    counters.sent.fetch_add(1, Ordering::Relaxed);
                } else {
                    counters.failed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
        Ok(Arc::new(Socket {
            inner,
            queue: tx,
            counters: self.counters.clone(),
            random: AtomicU64::new(self.seed.fetch_add(7919, Ordering::Relaxed)),
            delay: self.delay,
            active: self.active.clone(),
            next: tokio::sync::Mutex::new(tokio::time::Instant::now()),
        }))
    }
}
impl AsyncUdpSocket for Socket {
    fn send_to<'a>(
        &'a self,
        buf: &'a [u8],
        target: SocketAddr,
    ) -> Pin<Box<dyn Future<Output = io::Result<usize>> + Send + 'a>> {
        Box::pin(async move {
            if !self.active.load(Ordering::Relaxed) {
                return self.inner.send_to(buf, target).await;
            }
            self.counters.offered.fetch_add(1, Ordering::Relaxed);
            let old = self
                .random
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |mut x| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    Some(x)
                })
                .unwrap();
            if old % 100 == 0 {
                self.counters.dropped.fetch_add(1, Ordering::Relaxed);
                return Ok(buf.len());
            }
            // 8 MiB/s per socket, FIFO timing with a bounded queue (not a sleep per packet).
            let mut next = self.next.lock().await;
            let now = tokio::time::Instant::now();
            *next = (*next).max(now)
                + Duration::from_secs_f64(buf.len() as f64 / (8.0 * 1024.0 * 1024.0));
            let due = *next + self.delay;
            self.queue
                .send(Packet {
                    due,
                    body: buf.to_vec(),
                    target,
                })
                .await
                .map_err(|_| io::Error::other("impairment socket retired"))?;
            self.counters
                .peak_queue
                .fetch_max((8192 - self.queue.capacity()) as u64, Ordering::Relaxed);
            Ok(buf.len())
        })
    }
    fn recv_from<'a>(
        &'a self,
        buf: &'a mut [u8],
    ) -> Pin<Box<dyn Future<Output = io::Result<(usize, SocketAddr)>> + Send + 'a>> {
        self.inner.recv_from(buf)
    }
    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}
