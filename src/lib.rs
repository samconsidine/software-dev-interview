use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use udp_stream::{UdpListener, UdpStream};

/// Max bytes per underlying UDP `send_to` call.  The udp-stream crate uses a
/// 17 480-byte recv buffer internally, so we stay comfortably under that.
const CHUNK_SIZE: usize = 8192;

/// Lightweight wrapper around `udp_stream::UdpStream` that adds
/// simple length-prefixed framing so individual "frames" survive the
/// stream-oriented layer that the crate builds on top of raw UDP.
///
/// Wire format per frame:
///   [4 bytes BE length][payload]
pub struct UdpFrameStream {
    inner: UdpStream,
    stats: Arc<Stats>,
}

/// Cumulative byte counters.
#[derive(Debug, Default)]
pub struct Stats {
    pub bytes_sent: AtomicU64,
    pub bytes_recv: AtomicU64,
    pub frames_sent: AtomicU64,
    pub frames_recv: AtomicU64,
}

impl UdpFrameStream {
    /// Connect to a remote peer (client side).
    pub async fn connect(addr: SocketAddr) -> io::Result<Self> {
        let inner = UdpStream::connect(addr).await?;
        log::info!("connected to {addr}");
        Ok(Self {
            inner,
            stats: Arc::new(Stats::default()),
        })
    }

    /// Wrap an already-accepted `UdpStream` (server side).
    pub fn from_accepted(stream: UdpStream) -> Self {
        Self {
            inner: stream,
            stats: Arc::new(Stats::default()),
        }
    }

    /// Send a single framed message.
    ///
    /// Internally chunks the payload into UDP-safe pieces so callers
    /// can hand in arbitrarily large buffers.
    pub async fn send_frame(&mut self, data: &[u8]) -> io::Result<()> {
        let len = data.len() as u32;
        self.inner.write_all(&len.to_be_bytes()).await?;

        // Chunk the payload so each underlying sendto stays within
        // a safe UDP datagram size.
        for chunk in data.chunks(CHUNK_SIZE) {
            self.inner.write_all(chunk).await?;
        }

        self.stats
            .bytes_sent
            .fetch_add(data.len() as u64 + 4, Ordering::Relaxed);
        self.stats.frames_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Maximum frame size (10 MB) to guard against corrupted length prefixes.
    const MAX_FRAME_SIZE: usize = 10 * 1024 * 1024;

    /// Receive a single framed message.
    pub async fn recv_frame(&mut self) -> io::Result<Vec<u8>> {
        let mut len_buf = [0u8; 4];
        self.inner.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > Self::MAX_FRAME_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("frame too large: {len} bytes"),
            ));
        }

        let mut buf = vec![0u8; len];
        self.inner.read_exact(&mut buf).await?;
        self.stats
            .bytes_recv
            .fetch_add(len as u64 + 4, Ordering::Relaxed);
        self.stats.frames_recv.fetch_add(1, Ordering::Relaxed);
        Ok(buf)
    }

    pub fn stats(&self) -> Arc<Stats> {
        Arc::clone(&self.stats)
    }

    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.inner.peer_addr()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}

/// Wrapper around `udp_stream::UdpListener` that yields `UdpFrameStream`s.
pub struct UdpFrameListener {
    inner: UdpListener,
}

impl UdpFrameListener {
    pub async fn bind(addr: SocketAddr) -> io::Result<Self> {
        let inner = UdpListener::bind(addr).await?;
        log::info!("listening on {}", inner.local_addr()?);
        Ok(Self { inner })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    /// Accept the next peer, returning a framed stream.
    pub async fn accept(&self) -> io::Result<(UdpFrameStream, SocketAddr)> {
        let (stream, addr) = self.inner.accept().await?;
        log::info!("accepted peer {addr}");
        Ok((UdpFrameStream::from_accepted(stream), addr))
    }
}

// ---------------------------------------------------------------------------
// Test video source – generates raw RGB frames with an animated pattern
// ---------------------------------------------------------------------------

/// A synthetic test-pattern video source (colour bars + a moving stripe).
pub struct TestVideoSource {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    frame_no: u64,
}

impl TestVideoSource {
    pub fn new(width: u32, height: u32, fps: u32) -> Self {
        Self {
            width,
            height,
            fps,
            frame_no: 0,
        }
    }

    /// Generate the next raw RGB24 frame (3 bytes per pixel, row-major).
    /// First 8 bytes are a u64 BE timestamp (millis since epoch) for latency tracking.
    pub fn next_frame(&mut self) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut buf = vec![0u8; 8 + w * h * 3];

        // Embed sender timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        buf[..8].copy_from_slice(&now.to_be_bytes());

        let pixels = &mut buf[8..];

        // SMPTE-ish colour bars
        let bars: [(u8, u8, u8); 8] = [
            (192, 192, 192), // white/grey
            (192, 192, 0),   // yellow
            (0, 192, 192),   // cyan
            (0, 192, 0),     // green
            (192, 0, 192),   // magenta
            (192, 0, 0),     // red
            (0, 0, 192),     // blue
            (0, 0, 0),       // black
        ];

        let bar_w = w / bars.len();

        // Animated horizontal sweep line
        let sweep_y = (self.frame_no as usize * 3) % h;

        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) * 3;
                let bar_idx = (x / bar_w).min(bars.len() - 1);

                if y >= sweep_y && y < sweep_y + 4 {
                    pixels[idx] = 255;
                    pixels[idx + 1] = 255;
                    pixels[idx + 2] = 255;
                } else {
                    let (r, g, b) = bars[bar_idx];
                    pixels[idx] = r;
                    pixels[idx + 1] = g;
                    pixels[idx + 2] = b;
                }
            }
        }

        self.frame_no += 1;
        buf
    }

    pub fn frame_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(1.0 / self.fps as f64)
    }
}
