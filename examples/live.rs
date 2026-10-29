//! Connect to the first detector found and print live alerts until Ctrl-C.
//!
//! Run with: `cargo run --example live`

use std::time::Duration;

use futures::StreamExt;
use openrd::Event;

#[tokio::main]
async fn main() -> openrd::Result<()> {
    println!("scanning...");
    let detector = match openrd::scan(Duration::from_secs(5)).await?.into_iter().next() {
        Some(d) => d,
        None => {
            println!("no detector found. put it in pairing mode and try again.");
            return Ok(());
        }
    };

    println!("connecting to {}...", detector.kind().name());
    let session = detector.connect().await?;
    println!("connected. streaming alerts (Ctrl-C to quit):\n");

    let mut events = Box::pin(session.events());
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\ndisconnecting...");
                break;
            }
            event = events.next() => match event {
                Some(Event::Alert(a)) => println!("{a}"),
                Some(Event::EspPacket { id, payload }) => {
                    println!("esp packet id={id:#04x} payload={payload:02x?}");
                }
                Some(Event::Raw(bytes)) => println!("raw {bytes:02x?}"),
                None => break,
            }
        }
    }

    session.disconnect().await?;
    Ok(())
}
