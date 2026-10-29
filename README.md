# openrd

A cross-platform Rust client for Bluetooth LE radar detectors.

`openrd` scans for a detector, connects, streams decoded alerts, and (on Uniden)
writes settings and mutes. It is built on [`btleplug`], so the same code runs on
Windows, macOS and Linux with no platform-specific glue.

The point of this crate is to be the **open, documented client** these detectors
never shipped. The vendors expose Bluetooth but leave the app to third parties;
`openrd` is a reference implementation anyone can build an app, logger, or dash
display on top of. The wire protocols are written up in [`PROTOCOL.md`](PROTOCOL.md).

[`btleplug`]: https://docs.rs/btleplug

## Supported detectors

| Model | Alerts | Settings / mute | Protocol source |
|-------|:------:|:---------------:|-----------------|
| Uniden R4 / R8 / R9 (incl. `w`) | ✅ | ✅ | reverse-engineered |
| Valentine One Gen2 | ⚠️ best-effort | — | public ESP spec |
| Radenso DS1 | raw frames | — | scaffold, PRs welcome |

Adding a detector means implementing one `decode()` and listing its service
UUIDs; the BLE plumbing, scanning and event stream are shared.

## Install

```toml
[dependencies]
openrd = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
futures = "0.3"
```

## Usage

```rust
use futures::StreamExt;
use openrd::Event;
use std::time::Duration;

#[tokio::main]
async fn main() -> openrd::Result<()> {
    // Find the first detector advertising nearby (put it in BT pairing mode first).
    let detector = openrd::scan(Duration::from_secs(5))
        .await?
        .into_iter()
        .next()
        .expect("no detector found");

    println!("connecting to {}", detector.kind().name());
    let session = detector.connect().await?;

    // Live alerts.
    let mut events = Box::pin(session.events());
    while let Some(event) = events.next().await {
        if let Event::Alert(alert) = event {
            println!("{alert}"); // e.g. "Ka     34.700 GHz front front=12  rear=0   prio=1"
        }
    }
    Ok(())
}
```

Uniden settings and mute (see [`PROTOCOL.md`](PROTOCOL.md) for the index map):

```rust
session.mute().await?;              // mute the current alert
session.set_setting(1, 1).await?;   // write setting index=1 value=1 (City Mode -> City)
```

### Examples

```sh
cargo run --example replay # decode sample frames offline — no hardware needed
cargo run --example scan   # list detectors nearby
cargo run --example live   # connect to the first one and print alerts
```

**No detector on hand?** Start with `replay`. It runs the real decode path over
recorded/synthetic notification payloads and prints the alerts, so you can see
the parsers work (and extend them) without any Bluetooth hardware.

## How it works

- **Scan** uses the first system Bluetooth adapter and matches the advertised
  name (`R8`, `V1`, `DS1`, ...) to a model.
- **Connect** discovers services and picks the notify and write characteristics
  **by their GATT property flags**, not by hardcoded UUID roles. That keeps it
  working across firmware revisions that shuffle the characteristics.
- A background task pumps GATT notifications into a broadcast stream. On Uniden,
  command acknowledgements share the notify channel, so they are peeled off and
  matched to the command that is waiting.

## Legal / safety

Radar detectors are legal for passenger vehicles in most of the United States;
exceptions include Virginia, Washington D.C., and federal rules for commercial
vehicles over 10,000 lbs. Check your jurisdiction. This crate talks to a
detector you own over its own advertised Bluetooth interface. It does not
decrypt, jailbreak, or bypass anything. The Uniden protocol notes were obtained
by observing a detector's own Bluetooth output; the Valentine protocol is from
Valentine's published ESP specification.

Do not read alerts while driving in a way that distracts you. Keep your eyes on
the road.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
