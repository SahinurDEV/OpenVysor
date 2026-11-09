//! Needs a real, authorized Android device. Run with:
//! `cargo test --test real_device -- --ignored --nocapture --test-threads=1`

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use openvysor_lib::adb::device::{DeviceState, parse_devices};
use openvysor_lib::adb::{Adb, DEFAULT_TIMEOUT};
use openvysor_lib::mirroring::MirrorManager;
use openvysor_lib::mirroring::protocol::ControlMessage;
use openvysor_lib::mirroring::session::MirrorOptions;

const KEYCODE_SLEEP: u32 = 223;
const KEYCODE_WAKEUP: u32 = 224;

fn first_online(adb: &Adb) -> String {
    adb.start_server().unwrap();
    let output = adb.run_text(&["devices", "-l"], DEFAULT_TIMEOUT).unwrap();
    parse_devices(&output)
        .into_iter()
        .find(|device| device.state == DeviceState::Online)
        .expect("no online device")
        .serial
}

fn server_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/scrcpy-server")
}

fn wakefulness(adb: &Adb, serial: &str) -> String {
    let dump = adb.device_text(serial, &["shell", "dumpsys", "power"], DEFAULT_TIMEOUT).unwrap();
    dump.lines()
        .find_map(|line| line.trim().strip_prefix("mWakefulness="))
        .unwrap_or_default()
        .to_string()
}

fn press(manager: &MirrorManager, serial: &str, keycode: u32) {
    for action in [0u8, 1u8] {
        manager
            .send(serial, &ControlMessage::Keycode { action, keycode, repeat: 0, metastate: 0 })
            .unwrap();
    }
}

fn tap(manager: &MirrorManager, serial: &str, x: i32, y: i32, size: (u32, u32)) {
    for (action, pressure) in [(0u8, 1.0f32), (1u8, 0.0f32)] {
        manager
            .send(
                serial,
                &ControlMessage::Touch {
                    action,
                    pointer_id: -2,
                    x,
                    y,
                    width: size.0 as u16,
                    height: size.1 as u16,
                    pressure,
                    action_button: 0,
                    buttons: 0,
                },
            )
            .unwrap();
    }
}

struct StopOnDrop(Arc<MirrorManager>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.stop_all();
    }
}

#[test]
#[ignore = "requires a real Android device"]
fn streams_video_injects_input_and_cleans_up() {
    let adb = Adb::locate().unwrap();
    let serial = first_online(&adb);
    let initial = wakefulness(&adb, &serial);
    let manager = Arc::new(MirrorManager::new());
    let _guard = StopOnDrop(manager.clone());
    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    let session = manager
        .start(
            &adb,
            &serial,
            &serial,
            &server_path(),
            MirrorOptions::default(),
            None,
            Box::new(move |packet| tx.send(packet).is_ok()),
            Box::new(|reason| println!("ended: {reason}")),
        )
        .unwrap();
    println!("device name: {}", session.device_name());

    let (mut sessions, mut configs, mut keys, mut deltas) = (0, 0, 0, 0);
    let mut size = (0u32, 0u32);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let Ok(packet) = rx.recv_timeout(Duration::from_millis(200)) else { continue };
        match packet[0] {
            0 => {
                sessions += 1;
                size = (
                    u32::from_be_bytes(packet[9..13].try_into().unwrap()),
                    u32::from_be_bytes(packet[13..17].try_into().unwrap()),
                );
            }
            1 => configs += 1,
            2 => keys += 1,
            _ => deltas += 1,
        }
    }
    println!("sessions={sessions} configs={configs} keys={keys} deltas={deltas} size={size:?}");
    assert!(sessions >= 1 && configs >= 1 && keys >= 1, "missing stream packets");
    assert!(size.0 > 0 && size.1 > 0);

    // Sleep then wake gives a visible state change whether or not the phone is locked.
    press(&manager, &serial, KEYCODE_SLEEP);
    std::thread::sleep(Duration::from_millis(1500));
    let asleep = wakefulness(&adb, &serial);
    press(&manager, &serial, KEYCODE_WAKEUP);
    std::thread::sleep(Duration::from_millis(1500));
    let awake = wakefulness(&adb, &serial);
    println!("wakefulness: {initial} -> {asleep} -> {awake}");
    assert_ne!(asleep, "Awake", "KEYCODE_SLEEP was not injected");
    assert_eq!(awake, "Awake", "KEYCODE_WAKEUP was not injected");

    tap(&manager, &serial, size.0 as i32 / 2, size.1 as i32 / 8, size);
    std::thread::sleep(Duration::from_millis(500));
    let events = adb
        .device_text(&serial, &["shell", "dumpsys input | grep MotionEvent | tail -n 6"], DEFAULT_TIMEOUT)
        .unwrap();
    println!("recent motion events:
{events}");
    let recent = events
        .lines()
        .filter_map(|line| line.split("age=").nth(1)?.strip_suffix("ms")?.trim().parse::<u32>().ok())
        .filter(|age| *age < 2000)
        .count();
    assert!(recent >= 2, "tap did not reach the input dispatcher: {events}");

    if initial != "Awake" {
        press(&manager, &serial, KEYCODE_SLEEP);
        std::thread::sleep(Duration::from_millis(500));
    }

    manager.stop(&serial).unwrap();
    std::thread::sleep(Duration::from_millis(1500));

    let forwards = adb.run_text(&["forward", "--list"], DEFAULT_TIMEOUT).unwrap();
    assert!(!forwards.contains("scrcpy_"), "forward left behind: {forwards}");
    let server = adb
        .device_text(&serial, &["shell", "ps -ef | grep com.genymobile.scrcpy | grep -v grep || true"], DEFAULT_TIMEOUT)
        .unwrap();
    assert!(server.trim().is_empty(), "server still running: {server}");
}
