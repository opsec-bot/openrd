//! Offline decode harness: no Bluetooth, no hardware.
//!
//! Feeds recorded / synthetic detector notification payloads through the real
//! decode path ([`DetectorKind::decode`]) and prints the resulting events. This
//! is how you exercise and eyeball the parsers without owning a detector, and
//! it is the rig to extend when adding or fixing a protocol.
//!
//! Run with: `cargo run --example replay`

use openrd::{DetectorKind, Event};

fn main() {
    // --- Uniden R8: alert notifications (ASCII `&`-joined CSV records) -------
    // Format: index,count,BAND,frontStrength,rearStrength,freqGHz,DIR,priority
    let uniden_samples: &[(&str, &[u8])] = &[
        ("priority Ka threat ahead", b"1,1,KA,12,0,34.700,F,1"),
        (
            "K behind + Ka ahead at once",
            b"1,2,KA,9,0,33.800,F,1&2,2,K,5,3,24.150,R,2",
        ),
        ("empty slot then a laser hit", b"0,0,,0,0,0,0,0&1,1,LASER,30,0,0,F,1"),
        ("X band to the side", b"1,1,X,4,0,10.525,S,1"),
        // Replies to our own commands share the notify channel; decode drops them.
        ("settings-write ack (ignored)", b"RDrptSETC:1"),
        ("mute ack (ignored)", b"RDrptMUTE:1=1"),
    ];

    println!("== Uniden R8 ==");
    for (label, payload) in uniden_samples {
        print_decode(DetectorKind::UnidenR8, label, payload);
    }

    // --- Valentine One Gen2: ESP frame (AA .. AB) with an infDisplayData id --
    // AA dest orig id=0x31 len body... cksum AB. Body here: strength, _, bandbits
    // (0x02 = Ka), arrowbits (0x01 = front), _, _.
    let valentine_frame: &[u8] = &[
        0xAA, 0xD0, 0x0A, 0x31, 0x06, 0x0A, 0x00, 0x02, 0x01, 0x00, 0x00, 0x00, 0xAB,
    ];
    println!("\n== Valentine One Gen2 ==");
    print_decode(
        DetectorKind::ValentineV1Gen2,
        "infDisplayData: Ka, front, strength 10",
        valentine_frame,
    );
}

fn print_decode(kind: DetectorKind, label: &str, payload: &[u8]) {
    let events = kind.decode(payload);
    if events.is_empty() {
        println!("  {label:<32} -> (no events)");
        return;
    }
    for event in events {
        match event {
            Event::Alert(a) => println!("  {label:<32} -> {a}"),
            Event::EspPacket { id, payload } => {
                println!("  {label:<32} -> esp id={id:#04x} {payload:02x?}")
            }
            Event::Raw(bytes) => println!("  {label:<32} -> raw {bytes:02x?}"),
        }
    }
}
