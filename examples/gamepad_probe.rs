//! Sonde manette : décrit chaque manette vue par gilrs puis, pendant 20 s, compte les
//! événements **par manette** et par type. Usage : `cargo run --example gamepad_probe`
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

fn main() {
    let mut g = gilrs::Gilrs::new().expect("gilrs");
    let mut counts: BTreeMap<(String, String), u32> = BTreeMap::new();
    let mut described = std::collections::HashSet::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(60) {
        while let Some(ev) = g.next_event() {
            if described.insert(ev.id) {
                let pad = g.gamepad(ev.id);
                println!(
                    "{:?} : « {} » vendor={:04x?} product={:04x?} mapping={:?} power={:?}",
                    ev.id,
                    pad.name(),
                    pad.vendor_id(),
                    pad.product_id(),
                    pad.mapping_source(),
                    pad.power_info()
                );
            }
            let key = match ev.event {
                gilrs::EventType::ButtonPressed(b, _) => format!("Pressed {b:?}"),
                gilrs::EventType::ButtonReleased(b, _) => format!("Released {b:?}"),
                gilrs::EventType::ButtonChanged(b, _, _) => format!("Changed {b:?}"),
                gilrs::EventType::AxisChanged(a, _, _) => format!("Axis {a:?}"),
                other => format!("{other:?}"),
            };
            *counts.entry((format!("{:?}", ev.id), key)).or_default() += 1;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    println!("--- événements en 60 s, par manette ---");
    for ((id, k), v) in counts {
        println!("{v:>6}  pad {id}  {k}");
    }
}
