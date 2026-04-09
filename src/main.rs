use std::net::SocketAddr;
use std::sync::atomic::Ordering;

use clap::Parser;
use software_dev_interview::{TestVideoSource, UdpFrameStream};
use tokio::time;

#[derive(Parser)]
#[command(about = "Send test video frames over UDP")]
struct Args {
    /// Target address (bridge server)
    #[arg(short, long, default_value = "18.130.238.222:9001")]
    target: String,

    #[arg(long, default_value = "80")]
    width: u32,

    #[arg(long, default_value = "60")]
    height: u32,

    #[arg(long, default_value = "15")]
    fps: u32,

    /// Number of frames to send (0 = infinite)
    #[arg(short, long, default_value = "0")]
    count: u64,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();

    let addr: SocketAddr = args
        .target
        .parse()
        .or_else(|_| {
            use std::net::ToSocketAddrs;
            args.target
                .to_socket_addrs()
                .and_then(|mut addrs| addrs.next().ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::Other, "could not resolve address")
                }))
        })
        .expect("invalid target address");

    let mut stream = UdpFrameStream::connect(addr).await?;
    let mut source = TestVideoSource::new(args.width, args.height, args.fps);
    let stats = stream.stats();
    let interval = source.frame_interval();

    log::info!(
        "sending {}x{} RGB24 @ {} fps → {}",
        args.width,
        args.height,
        args.fps,
        addr
    );

    let mut ticker = time::interval(interval);
    let mut frame_no = 0u64;

    loop {
        if args.count > 0 && frame_no >= args.count {
            break;
        }

        ticker.tick().await;
        let frame = source.next_frame();
        stream.send_frame(&frame).await?;

        if frame_no % (args.fps as u64) == 0 {
            log::info!(
                "frame {frame_no} | {:.1} MB sent",
                stats.bytes_sent.load(Ordering::Relaxed) as f64 / 1_048_576.0,
            );
        }
        frame_no += 1;
    }

    log::info!("done – sent {frame_no} frames");
    Ok(())
}
