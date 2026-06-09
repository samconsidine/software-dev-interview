use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::AsyncWriteExt;
use udp_stream::UdpStream;

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
    pub frames_sent: AtomicU64,
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

    /// Send a single framed message.
    ///
    /// Internally chunks the payload into UDP-safe pieces so callers
    /// can hand in arbitrarily large buffers.
    pub async fn send_frame(&mut self, data: &[u8]) -> io::Result<()> {
        let len = data.len() as u32;

        if data.len() + 4 <= CHUNK_SIZE {
            let mut packet = Vec::with_capacity(data.len() + 4);
            packet.extend_from_slice(&len.to_be_bytes());
            packet.extend_from_slice(data);
            self.inner.write_all(&packet).await?;
        } else {
            self.inner.write_all(&len.to_be_bytes()).await?;

            // Chunk the payload so each underlying sendto stays within
            // a safe UDP datagram size.
            for chunk in data.chunks(CHUNK_SIZE) {
                self.inner.write_all(chunk).await?;
            }
        }

        self.stats
            .bytes_sent
            .fetch_add(data.len() as u64 + 4, Ordering::Relaxed);
        self.stats.frames_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn stats(&self) -> Arc<Stats> {
        Arc::clone(&self.stats)
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
    pub fn next_frame(&mut self) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        let rgb_len = w * h * 3;
        let mut buf = vec![0u8; rgb_len];

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

        let bar_w = (w / bars.len()).max(1);

        // Animated horizontal sweep line
        let sweep_y = if h > 0 {
            (self.frame_no as usize * 3) % h
        } else {
            0
        };

        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) * 3;
                let bar_idx = (x / bar_w).min(bars.len() - 1);

                if y >= sweep_y && y < sweep_y + 4 {
                    buf[idx] = 255;
                    buf[idx + 1] = 255;
                    buf[idx + 2] = 255;
                } else {
                    let (r, g, b) = bars[bar_idx];
                    buf[idx] = r;
                    buf[idx + 1] = g;
                    buf[idx + 2] = b;
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
