//! Needs one authorized phone. It opens Calculator on a private virtual display,
//! so nothing on the phone's own screen is captured.
//!
//! cargo test --release --test real_device -- --ignored --nocapture

use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::ColorImage;
use openvysor_native::adb::Adb;
use openvysor_native::mirroring::protocol::ControlMessage;
use openvysor_native::mirroring::session::MirrorOptions;
use openvysor_native::mirroring::{MirrorManager, StartRequest};
use openvysor_native::video::{Video, packet_sink};

const CALCULATOR: &str = "com.google.android.calculator";
const KEY: &str = "native-test";
/// The "7" button on a 720 by 1600 display.
const SEVEN: (i32, i32) = (90, 939);

fn tap(manager: &MirrorManager, (x, y): (i32, i32), size: [usize; 2]) {
    for (action, pressure) in [(0, 1.0), (1, 0.0)] {
        let message = ControlMessage::Touch {
            action,
            pointer_id: -2,
            x,
            y,
            width: size[0] as u16,
            height: size[1] as u16,
            pressure,
            action_button: 0,
            buttons: 0,
        };
        manager.send(KEY, &message).unwrap();
        thread::sleep(Duration::from_millis(60));
    }
}

fn newest_after(video: &Video, wait: Duration, mut image: Option<ColorImage>) -> Option<ColorImage> {
    let started = Instant::now();
    while started.elapsed() < wait {
        if let Some(latest) = video.take_latest() {
            image = Some(latest);
        }
        thread::sleep(Duration::from_millis(5));
    }
    image
}

fn first_online_serial(adb: &Adb) -> String {
    let output = adb.run_text(&["devices"], Duration::from_secs(10)).unwrap();
    output
        .lines()
        .skip(1)
        .find_map(|line| line.strip_suffix("\tdevice").map(str::to_string))
        .expect("no authorized phone is connected")
}

#[test]
#[ignore = "needs a real phone"]
fn decodes_a_real_stream() {
    env_logger::builder().filter_level(log::LevelFilter::Warn).init();
    let adb = Adb::locate().expect("adb was not found");
    let serial = first_online_serial(&adb);
    let manager = Arc::new(MirrorManager::new());
    let video = Arc::new(Video::default());
    let sink = packet_sink(video.clone(), Arc::new(|| {}), None);

    let started = Instant::now();
    let request = StartRequest {
        key: KEY,
        serial: &serial,
        server_path: Path::new("resources/scrcpy-server"),
        options: MirrorOptions::default(),
        start_app: Some(CALCULATOR),
    };
    manager.start(&adb, request, sink, Box::new(|reason| println!("ended: {reason}"))).unwrap();

    let mut first_picture = None;
    let mut image = None;
    while started.elapsed() < Duration::from_secs(10) {
        if let Some(latest) = video.take_latest() {
            first_picture.get_or_insert(started.elapsed());
            image = Some(latest);
            if video.frames() >= 3 {
                break;
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    // Let the app finish drawing, then check that a tap changes its display.
    let settled = newest_after(&video, Duration::from_secs(2), image).expect("no picture was decoded");
    tap(&manager, SEVEN, settled.size);
    let tapped = newest_after(&video, Duration::from_secs(2), None);
    let image = Some(settled);
    let error = video.take_error();
    manager.stop_all();

    assert_eq!(error, None);
    let image = image.expect("no picture was decoded");
    let lit = image.pixels.iter().filter(|pixel| pixel.r() > 16 || pixel.g() > 16 || pixel.b() > 16).count();
    println!(
        "size={:?} frames={} first picture after {:?} lit pixels={lit} of {}",
        image.size,
        video.frames(),
        first_picture.unwrap(),
        image.pixels.len()
    );
    assert_eq!(image.size.map(|side| side as u32), video.size());
    assert!(lit > image.pixels.len() / 10, "the picture is almost entirely black");

    let tapped = tapped.expect("the tap did not produce a new frame");
    let [width, _] = image.size;
    let changed = (100..500)
        .flat_map(|row| (0..width).map(move |column| row * width + column))
        .filter(|&index| image.pixels[index] != tapped.pixels[index])
        .count();
    println!("pixels changed in the number display: {changed}");
    assert!(changed > 100, "tapping 7 did not change the number display");
}
