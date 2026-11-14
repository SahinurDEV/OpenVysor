//! Needs one authorized phone. It opens Calculator on a private virtual display,
//! so nothing on the phone's own screen is captured.
//!
//! cargo test --release --test real_device -- --ignored --nocapture

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::ColorImage;
use openvysor_native::adb::Adb;
use openvysor_native::decoder::{Decoder, nv12_to_pixels};
use openvysor_native::mirroring::protocol::ControlMessage;
use openvysor_native::mirroring::session::{MirrorOptions, PacketSink};
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

fn timed<T>(label: &str, action: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = action();
    println!("{label}: {:?}", started.elapsed());
    result
}

fn mean(durations: &[Duration]) -> Duration {
    durations.iter().sum::<Duration>() / durations.len().max(1) as u32
}

/// Prints where the time goes when a window opens, shows frames and closes.
#[test]
#[ignore = "needs a real phone"]
fn reports_timings() {
    const SCRATCH_JAR: &str = "/data/local/tmp/openvysor-timing.jar";
    const SCRATCH_COPY: &str = "/data/local/tmp/openvysor-timing-copy.jar";
    let adb = Adb::locate().expect("adb was not found");
    let serial = first_online_serial(&adb);
    let timeout = Duration::from_secs(30);

    timed("adb start-server", || adb.start_server().unwrap());
    timed("adb get-state", || adb.device_text(&serial, &["get-state"], timeout).unwrap());
    let push = ["-s", &serial, "push", "resources/scrcpy-server", SCRATCH_JAR];
    timed("adb push server", || adb.run(&push, timeout).unwrap());
    timed("adb shell cp", || adb.device_text(&serial, &["shell", "cp", SCRATCH_JAR, SCRATCH_COPY], timeout).unwrap());
    adb.device_text(&serial, &["shell", "rm", SCRATCH_JAR, SCRATCH_COPY], timeout).unwrap();

    let manager = Arc::new(MirrorManager::new());
    let packets = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    for round in 0..3 {
        let video = Arc::new(Video::default());
        let inner = packet_sink(video.clone(), Arc::new(|| {}), None);
        let captured = packets.clone();
        let sink: PacketSink = Arc::new(move |packet: Vec<u8>| {
            captured.lock().unwrap().push(packet.clone());
            inner(packet)
        });
        let request = StartRequest {
            key: KEY,
            serial: &serial,
            server_path: Path::new("resources/scrcpy-server"),
            options: MirrorOptions { max_fps: 120, ..MirrorOptions::default() },
            start_app: Some(CALCULATOR),
        };
        let started = Instant::now();
        manager.start(&adb, request, sink, Box::new(|_| {})).unwrap();
        let connected = started.elapsed();
        while video.take_latest().is_none() && started.elapsed() < Duration::from_secs(10) {
            thread::sleep(Duration::from_millis(1));
        }
        println!("round {round}: connected {connected:?}, first picture {:?}", started.elapsed());
        thread::sleep(Duration::from_millis(1500));
        let [width, height] = video.size().map(|side| side as usize);
        for _ in 0..8 {
            tap(&manager, SEVEN, [width, height]);
        }
        thread::sleep(Duration::from_millis(300));
        timed("stop", || manager.stop(KEY).unwrap());
    }

    let (mut size, mut decode_times) = ([0usize; 2], Vec::new());
    let mut decoder = Decoder::new().unwrap();
    let mut pixels = Vec::new();
    for packet in packets.lock().unwrap().iter() {
        let payload = &packet[9..];
        match packet[0] {
            0 => size = [0, 4].map(|at| u32::from_be_bytes(payload[at..at + 4].try_into().unwrap()) as usize),
            2 | 3 => {
                let started = Instant::now();
                let _ = decoder.decode(payload, 0);
                decode_times.push(started.elapsed());
            }
            _ => {}
        }
    }
    println!(
        "decode at {size:?}: mean {:?}, max {:?}, {} frames",
        mean(&decode_times),
        decode_times.iter().max().unwrap(),
        decode_times.len()
    );

    for (width, height, step) in [(720usize, 1600usize, 1usize), (1080, 2392, 1), (1080, 2392, 2)] {
        let nv12 = vec![128u8; width * height * 3 / 2];
        let times: Vec<_> = (0..20)
            .map(|_| {
                let started = Instant::now();
                nv12_to_pixels(&nv12, width, height, width, height, step, false, &mut pixels);
                started.elapsed()
            })
            .collect();
        println!("colour conversion at {width}x{height} step {step}: mean {:?} ({} pixels)", mean(&times), pixels.len());
    }

    let leftovers = adb.device_text(&serial, &["shell", "ls", "/data/local/tmp"], timeout).unwrap();
    let jars: Vec<_> = leftovers.lines().filter(|name| name.starts_with("openvysor-")).collect();
    println!("jars on the phone: {jars:?}");
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
    let error = video.take_error();
    manager.stop_all();

    assert_eq!(error, None);
    let image = settled;
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
