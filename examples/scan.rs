//! List every supported detector advertising nearby.
//!
//! Run with: `cargo run --example scan`

use std::time::Duration;

#[tokio::main]
async fn main() -> openrd::Result<()> {
    println!("scanning for 5s...");
    let detectors = openrd::scan(Duration::from_secs(5)).await?;

    if detectors.is_empty() {
        println!("no detectors found. make sure the detector is in pairing mode.");
        return Ok(());
    }

    for d in &detectors {
        println!("  {}  (advertised as {:?})", d.kind().name(), d.name());
    }
    Ok(())
}
