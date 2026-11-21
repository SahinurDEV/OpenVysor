//! Drives the real phone window, without showing it, against one authorized phone.
//!
//! Each test sends the same mouse and keyboard events a person would, through
//! `Phone::show`, then checks what happened on the phone with `uiautomator` and
//! `dumpsys`. Settings go to a temporary folder, never the user's own.
//!
//! The phone must be awake and unlocked. Run one test at a time:
//!
//! cargo test --release --test ui_device -- --ignored --nocapture --test-threads=1

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::accesskit::Role;
use eframe::egui::{
    Context, DroppedFile, Event, FullOutput, Key, Modifiers, MouseWheelSource, MouseWheelUnit,
    PointerButton, Pos2, RawInput, Rect, TouchPhase, Ui, ViewportCommand, ViewportId, pos2, vec2,
};
use openvysor_native::adb::Adb;
use openvysor_native::files_page::FilesPage;
use openvysor_native::notifications_page::NotificationsPage;
use openvysor_native::phone::Phone;
use openvysor_native::profiles::{KeyBinding, Quality};
use openvysor_native::state::Core;

/// Calculator differs by maker: its package, launch activity, and the ids of 7, clear and the formula.
struct Calculator {
    package: &'static str,
    activity: &'static str,
    seven: &'static str,
    clear: &'static str,
    formula: &'static str,
}

const CALCULATORS: [Calculator; 2] = [
    Calculator {
        package: "com.google.android.calculator",
        activity: "com.google.android.calculator/com.android.calculator2.Calculator",
        seven: "com.google.android.calculator:id/digit_7",
        clear: "com.google.android.calculator:id/clr",
        formula: "com.google.android.calculator:id/formula",
    },
    Calculator {
        package: "com.sec.android.app.popupcalculator",
        activity: "com.sec.android.app.popupcalculator/.Calculator",
        seven: "com.sec.android.app.popupcalculator:id/calc_keypad_btn_07",
        clear: "com.sec.android.app.popupcalculator:id/calc_keypad_btn_clear",
        formula: "com.sec.android.app.popupcalculator:id/calc_edt_formula",
    },
];
/// Built-in video players that can play the test tone, as package/activity.
const PLAYERS: [&str; 2] = [
    "com.samsung.android.video/.player.activity.MoviePlayer",
    "com.google.android.apps.photos/.pager.HostPhotoPagerActivity",
];

const CLOCKS: [&str; 2] = [
    "com.google.android.deskclock",
    "com.sec.android.app.clockpackage",
];
const WINDOW: [f32; 2] = [420.0, 930.0];
const WAIT: Duration = Duration::from_secs(20);

// ---- The phone, through adb ----

struct Device {
    adb: Adb,
    serial: String,
    calculator: &'static Calculator,
    clock: &'static str,
}

impl Device {
    fn connect() -> Self {
        let adb = Adb::locate().expect("adb was not found");
        let output = adb.run_text(&["devices"], Duration::from_secs(10)).unwrap();
        // With several phones connected, OPENVYSOR_TEST_SERIAL picks the one to use.
        let wanted = std::env::var("OPENVYSOR_TEST_SERIAL").ok();
        let serial = output
            .lines()
            .skip(1)
            .filter_map(|line| line.strip_suffix("\tdevice"))
            .find(|serial| wanted.as_deref().is_none_or(|wanted| wanted == *serial))
            .expect("no authorized phone is connected")
            .to_string();
        let packages = adb
            .device_text(
                &serial,
                &["shell", "pm list packages"],
                Duration::from_secs(30),
            )
            .unwrap();
        let has = |package: &str| {
            packages
                .lines()
                .any(|line| line.trim() == format!("package:{package}"))
        };
        let calculator = CALCULATORS
            .iter()
            .find(|calculator| has(calculator.package))
            .expect("no known Calculator app");
        let clock = CLOCKS
            .into_iter()
            .find(|clock| has(clock))
            .expect("no known Clock app");
        let device = Self {
            adb,
            serial,
            calculator,
            clock,
        };
        device.shell("cmd statusbar collapse; input keyevent KEYCODE_WAKEUP");
        device
    }

    fn shell(&self, command: &str) -> String {
        self.adb
            .device_text(&self.serial, &["shell", command], Duration::from_secs(30))
            .unwrap_or_default()
    }

    /// The main display's views, as `uiautomator` sees them.
    fn views(&self) -> String {
        self.shell("uiautomator dump /sdcard/openvysor-ui.xml >/dev/null && cat /sdcard/openvysor-ui.xml; rm -f /sdcard/openvysor-ui.xml")
    }

    /// The text of the first view with this resource id.
    fn text_of(&self, id: &str) -> Option<String> {
        attribute(
            &node(&self.views(), &format!("resource-id=\"{id}\""))?,
            "text",
        )
    }

    /// The centre of the first view whose attributes contain `pattern`, in display pixels.
    fn centre_of(&self, pattern: &str) -> Option<(f32, f32)> {
        let bounds = attribute(&node(&self.views(), pattern)?, "bounds")?;
        let numbers: Vec<f32> = bounds
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect();
        Some((
            (numbers[0] + numbers[2]) / 2.0,
            (numbers[1] + numbers[3]) / 2.0,
        ))
    }

    fn display_size(&self) -> (f32, f32) {
        let text = self.shell("wm size");
        let size = text
            .lines()
            .last()
            .and_then(|line| line.rsplit(' ').next())
            .unwrap()
            .trim()
            .to_string();
        let (w, h) = size.split_once('x').unwrap();
        (w.parse().unwrap(), h.parse().unwrap())
    }

    fn top_package(&self) -> String {
        let text = self.shell("dumpsys activity activities | grep -m1 topResumedActivity");
        text.split_whitespace()
            .find(|word| word.contains('/'))
            .and_then(|word| word.split('/').next())
            .unwrap_or_default()
            .to_string()
    }

    fn open_calculator(&self) {
        self.shell("cmd statusbar collapse");
        self.shell(&format!(
            "am start -W -n {} >/dev/null",
            self.calculator.activity
        ));
        thread::sleep(Duration::from_millis(800));
        // Clears the formula with the app's own button, if anything is entered.
        if let Some(centre) = self.centre_of(&format!("resource-id=\"{}\"", self.calculator.clear))
        {
            self.shell(&format!("input tap {} {}", centre.0, centre.1));
        }
        for _ in 0..12 {
            self.shell("input keyevent KEYCODE_DEL");
        }
        thread::sleep(Duration::from_millis(300));
    }

    fn formula(&self) -> String {
        self.text_of(self.calculator.formula).unwrap_or_default()
    }

    fn seven(&self) -> Option<(f32, f32)> {
        self.centre_of(&format!("resource-id=\"{}\"", self.calculator.seven))
    }

    fn wait_for(&self, what: &str, mut ready: impl FnMut() -> bool) {
        let started = Instant::now();
        while !ready() {
            assert!(started.elapsed() < WAIT, "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(300));
        }
    }
}

fn node(xml: &str, pattern: &str) -> Option<String> {
    let at = xml.find(pattern)?;
    let start = xml[..at].rfind("<node")?;
    let end = at + xml[at..].find('>')?;
    Some(xml[start..end].to_string())
}

fn attribute(node: &str, name: &str) -> Option<String> {
    let marker = format!(" {name}=\"");
    let start = node.find(&marker)? + marker.len();
    let end = start + node[start..].find('"')?;
    Some(node[start..end].replace("&amp;", "&"))
}

// ---- The app, without a window ----

struct Window {
    ctx: Context,
    core: Arc<Core>,
    phone: Arc<Phone>,
    started: Instant,
    buttons: Vec<(String, Rect)>,
    /// Commands the last frame sent to windows, such as Focus.
    commands: Vec<(ViewportId, ViewportCommand)>,
}

#[derive(Debug)]
struct LocalFile(PathBuf);

impl DroppedFile for LocalFile {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|error| error.to_string())
    }
}

/// A settings folder of its own, so a test never reads or changes the user's.
fn new_core(test: &str) -> (Context, Arc<Core>) {
    let folder = std::env::temp_dir().join(format!("openvysor-ui-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    // Safe: each test sets it before any other thread of the app starts.
    unsafe { std::env::set_var("OPENVYSOR_DATA_DIR", &folder) };
    let ctx = Context::default();
    ctx.enable_accesskit();
    // Like eframe with a real backend: each phone is a window of its own.
    ctx.set_embed_viewports(false);
    let core = Core::new(&ctx);
    let started = Instant::now();
    while core.registry.devices().is_empty() {
        assert!(started.elapsed() < WAIT, "the app never saw the phone");
        thread::sleep(Duration::from_millis(100));
    }
    (ctx, core)
}

/// Remembers where each named button was drawn, from the frame's accessibility tree.
/// A text field is remembered as "text field".
fn collect_buttons(output: FullOutput, buttons: &mut Vec<(String, Rect)>) {
    let Some(update) = output.platform_output.accesskit_update else {
        return;
    };
    for (_, node) in update.nodes {
        let label = match node.role() {
            Role::Button => node.label().map(str::to_string),
            Role::TextInput => Some("text field".to_string()),
            _ => None,
        };
        if let (Some(label), Some(bounds)) = (label, node.bounds()) {
            let rect = Rect::from_min_max(
                pos2(bounds.x0 as f32, bounds.y0 as f32),
                pos2(bounds.x1 as f32, bounds.y1 as f32),
            );
            buttons.retain(|(known, _)| *known != label);
            buttons.push((label, rect));
        }
    }
}

/// A page of the main window, such as Files or Notifications, run without showing it.
struct Page {
    ctx: Context,
    started: Instant,
    buttons: Vec<(String, Rect)>,
}

impl Page {
    fn new(ctx: &Context) -> Self {
        Self {
            ctx: ctx.clone(),
            started: Instant::now(),
            buttons: Vec::new(),
        }
    }

    fn frame(&mut self, events: Vec<Event>, dropped: Vec<PathBuf>, show: &mut dyn FnMut(&mut Ui)) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 1400.0))),
            time: Some(self.started.elapsed().as_secs_f64()),
            events,
            dropped_files: dropped
                .into_iter()
                .map(|path| Arc::new(LocalFile(path)) as _)
                .collect(),
            ..RawInput::default()
        };
        let output = self.ctx.run_ui(input, |ui| show(ui));
        collect_buttons(output, &mut self.buttons);
    }

    fn has(&self, label: &str) -> bool {
        self.buttons.iter().any(|(known, _)| known == label)
    }

    /// Runs frames until a button with this name appears.
    fn wait_for(&mut self, label: &str, show: &mut dyn FnMut(&mut Ui)) {
        let started = Instant::now();
        while !self.has(label) {
            assert!(started.elapsed() < WAIT, "no button named {label} appeared");
            self.frame(Vec::new(), Vec::new(), show);
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn click(&mut self, label: &str, show: &mut dyn FnMut(&mut Ui)) {
        self.wait_for(label, show);
        let at = self
            .buttons
            .iter()
            .find(|(known, _)| known == label)
            .unwrap()
            .1
            .center();
        // The page changes after a click, so names seen before it are forgotten.
        self.buttons.clear();
        self.frame(vec![Event::PointerMoved(at)], Vec::new(), show);
        for pressed in [true, false] {
            let event = Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![event], Vec::new(), show);
        }
    }
}

impl Window {
    fn open(ctx: &Context, core: &Arc<Core>, serial: &str, package: Option<&str>) -> Self {
        core.open_phone(ctx, serial, package.map(str::to_string), "Test".to_string());
        let phone = core.phones().into_iter().last().unwrap();
        let mut window = Self {
            ctx: ctx.clone(),
            core: core.clone(),
            phone,
            started: Instant::now(),
            buttons: Vec::new(),
            commands: Vec::new(),
        };
        window.wait_until_live();
        window
    }

    fn wait_until_live(&mut self) {
        let started = Instant::now();
        loop {
            self.frame(Vec::new(), Vec::new());
            if self.phone.status_text().1
                && self.phone.picture_rect().is_some()
                && self.phone.video.frames() > 2
            {
                break;
            }
            assert!(
                started.elapsed() < WAIT,
                "the window never showed the phone: {:?}",
                self.phone.status_text()
            );
            thread::sleep(Duration::from_millis(30));
        }
        self.idle(Duration::from_millis(800));
    }

    /// Runs one frame of the window with these events, as eframe would.
    fn frame(&mut self, events: Vec<Event>, dropped: Vec<PathBuf>) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(WINDOW[0], WINDOW[1]))),
            time: Some(self.started.elapsed().as_secs_f64()),
            events,
            dropped_files: dropped
                .into_iter()
                .map(|path| Arc::new(LocalFile(path)) as _)
                .collect(),
            ..RawInput::default()
        };
        let phone = self.phone.clone();
        let others: Vec<Arc<Phone>> = self
            .core
            .phones()
            .into_iter()
            .filter(|other| other.id != phone.id)
            .collect();
        let output = self.ctx.run_ui(input, |ui| {
            // As in the app, the other phones are windows of their own, so commands can reach them.
            for other in &others {
                ui.ctx()
                    .show_viewport_deferred(other.id, other.viewport_builder(), |_, _| {});
            }
            phone.show(ui);
        });
        self.commands = output
            .viewport_output
            .iter()
            .flat_map(|(id, viewport)| {
                viewport
                    .commands
                    .iter()
                    .map(|command| (*id, command.clone()))
            })
            .collect();
        collect_buttons(output, &mut self.buttons);
    }

    fn idle(&mut self, time: Duration) {
        let started = Instant::now();
        while started.elapsed() < time {
            self.frame(Vec::new(), Vec::new());
            thread::sleep(Duration::from_millis(16));
        }
    }

    /// Where a point on the phone's display appears in the window.
    fn at(&self, device: &Device, (x, y): (f32, f32)) -> Pos2 {
        let rect = self.phone.picture_rect().unwrap();
        let (width, height) = device.display_size();
        let landscape = rect.width() > rect.height();
        let (width, height) = if landscape == (width > height) {
            (width, height)
        } else {
            (height, width)
        };
        pos2(
            rect.left() + x / width * rect.width(),
            rect.top() + y / height * rect.height(),
        )
    }

    fn button(&mut self, button: PointerButton, at: Pos2, pressed: bool) {
        let events = vec![
            Event::PointerMoved(at),
            Event::PointerButton {
                pos: at,
                button,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ];
        self.frame(events, Vec::new());
    }

    fn click(&mut self, button: PointerButton, at: Pos2) {
        self.button(button, at, true);
        thread::sleep(Duration::from_millis(60));
        self.button(button, at, false);
        self.idle(Duration::from_millis(300));
    }

    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.button(PointerButton::Primary, from, true);
        for step in 1..=12 {
            let at = from + (to - from) * (step as f32 / 12.0);
            self.frame(vec![Event::PointerMoved(at)], Vec::new());
            thread::sleep(Duration::from_millis(16));
        }
        self.button(PointerButton::Primary, to, false);
        self.idle(Duration::from_millis(500));
    }

    fn wheel(&mut self, at: Pos2, lines: f32) {
        self.frame(vec![Event::PointerMoved(at)], Vec::new());
        let event = Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: vec2(0.0, lines),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![event], Vec::new());
        self.idle(Duration::from_millis(600));
    }

    fn key(&mut self, key: Key) {
        for pressed in [true, false] {
            let event = Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![event], Vec::new());
            thread::sleep(Duration::from_millis(40));
        }
        self.idle(Duration::from_millis(300));
    }

    fn key_and_text(&mut self, key: Key, text: &str) {
        let down = Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let up = Event::Key {
            key,
            physical_key: Some(key),
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![down, Event::Text(text.to_string())], Vec::new());
        thread::sleep(Duration::from_millis(40));
        self.frame(vec![up], Vec::new());
        self.idle(Duration::from_millis(300));
    }

    fn type_text(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.to_string())], Vec::new());
        self.idle(Duration::from_millis(500));
    }

    fn press_toolbar(&mut self, label: &str) {
        let rect = self
            .buttons
            .iter()
            .find(|(known, _)| known == label)
            .map(|(_, rect)| *rect);
        let rect = rect.unwrap_or_else(|| {
            panic!(
                "no toolbar button named {label}; have {:?}",
                self.buttons.iter().map(|(l, _)| l).collect::<Vec<_>>()
            )
        });
        self.click(PointerButton::Primary, rect.center());
    }

    fn close(mut self) {
        self.phone.close();
        self.idle(Duration::from_millis(300));
        self.core.mirrors.stop_all();
    }
}

/// This phone's tunnels and running servers. Another copy of the app may own some, so
/// tests compare before and after rather than expecting none.
fn sessions(device: &Device) -> std::collections::BTreeSet<String> {
    let forwards = device
        .adb
        .run_text(&["forward", "--list"], Duration::from_secs(10))
        .unwrap_or_default();
    let servers = device.shell("ps -A -o ARGS | grep '^app_process.*scrcpy.Server'");
    forwards
        .lines()
        .filter(|line| line.starts_with(&device.serial))
        .map(str::to_string)
        .chain(servers.lines().filter_map(|line| {
            line.split_whitespace()
                .find(|word| word.starts_with("scid="))
                .map(str::to_string)
        }))
        .collect()
}

fn assert_nothing_left(device: &Device, before: &std::collections::BTreeSet<String>) {
    let started = Instant::now();
    loop {
        let left: Vec<String> = sessions(device).difference(before).cloned().collect();
        if left.is_empty() {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "left behind on the phone: {left:?}"
        );
        thread::sleep(Duration::from_millis(500));
    }
}

// ---- Tests ----

#[test]
#[ignore = "needs a real phone"]
fn whole_phone_taps_types_and_presses_back_and_home() {
    let device = Device::connect();
    device.open_calculator();
    let seven = device.seven().expect("Calculator's 7 not found");
    let before = sessions(&device);
    let (ctx, core) = new_core("input");
    let mut window = Window::open(&ctx, &core, &device.serial, None);

    let at = window.at(&device, seven);
    window.click(PointerButton::Primary, at);
    device.wait_for("a click on 7", || device.formula().contains('7'));
    println!("click: formula is {:?}", device.formula());

    // Calculator doubles injected characters, even from Android's own `input text`, so
    // typing is checked in a plain text field: Settings search.
    let search = "id/search_src_text\"";
    device.shell("am start -W -a com.android.settings.action.SETTINGS_SEARCH >/dev/null");
    device.wait_for("Settings search", || device.centre_of(search).is_some());
    let field = device.centre_of(search).unwrap();
    window.idle(Duration::from_millis(500));
    window.click(PointerButton::Primary, window.at(&device, field));
    device.shell("input keyevent KEYCODE_MOVE_END; for i in 1 2 3 4 5 6 7 8 9 10 11 12; do input keyevent KEYCODE_DEL; done");
    let typed = || {
        node(&device.views(), search)
            .and_then(|field| attribute(&field, "text"))
            .unwrap_or_default()
    };
    // A real keyboard sends the key and then the character, as egui reports them.
    window.type_text("wi");
    window.key_and_text(Key::F, "f");
    window.type_text("i");
    device.wait_for("typing", || {
        let text = typed();
        println!("  field: {text:?}");
        text == "wifi"
    });
    println!("typing: field is {:?}", typed());
    window.key(Key::Backspace);
    device.wait_for("Backspace", || typed() == "wif");
    println!("Backspace: field is {:?}", typed());
    window.key(Key::Escape);
    device.open_calculator();
    window.idle(Duration::from_millis(600));

    // Samsung's Calculator ignores Back, so Back is checked on the Wi-Fi settings page.
    device.shell(
        "input keyevent KEYCODE_HOME; am start -W -a android.settings.WIFI_SETTINGS >/dev/null",
    );
    device.wait_for("Wi-Fi settings", || {
        device.top_package() == "com.android.settings"
    });
    window.idle(Duration::from_millis(600));
    window.click(PointerButton::Secondary, at);
    device.wait_for("a right click to go back", || {
        device.top_package() != "com.android.settings"
    });
    println!("right click: top app is {}", device.top_package());

    device.open_calculator();
    window.idle(Duration::from_millis(600));
    window.click(PointerButton::Middle, at);
    device.wait_for("a middle click to go home", || {
        device.top_package() != device.calculator.package
    });
    println!("middle click: top app is {}", device.top_package());

    for (label, expect_home) in [("Recent apps", false), ("Home", true)] {
        device.open_calculator();
        window.idle(Duration::from_millis(600));
        window.press_toolbar(label);
        device.wait_for(label, || {
            let top = device.top_package();
            top != device.calculator.package && (!expect_home || !top.is_empty())
        });
        println!("toolbar {label}: top app is {}", device.top_package());
    }
    device.shell("input keyevent KEYCODE_HOME");
    window.close();
    assert_nothing_left(&device, &before);
    println!("nothing left on the phone");
}

#[test]
#[ignore = "needs a real phone"]
fn whole_phone_drags_and_scrolls() {
    let device = Device::connect();
    device.shell("am start -W -a android.settings.SETTINGS >/dev/null");
    thread::sleep(Duration::from_secs(1));
    let first_title = || {
        let views = device.views();
        let marker = "resource-id=\"android:id/title\"";
        node(&views, marker)
            .and_then(|node| attribute(&node, "text"))
            .unwrap_or_default()
    };
    let (ctx, core) = new_core("drag");
    let mut window = Window::open(&ctx, &core, &device.serial, None);
    let (width, height) = device.display_size();

    let before = first_title();
    let from = window.at(&device, (width / 2.0, height * 0.75));
    let to = window.at(&device, (width / 2.0, height * 0.30));
    window.drag(from, to);
    let after_drag = first_title();
    println!("drag: first item {before:?} -> {after_drag:?}");
    assert_ne!(before, after_drag, "dragging did not scroll Settings");

    window.wheel(from, -5.0);
    let after_wheel = first_title();
    println!("wheel down: first item {after_drag:?} -> {after_wheel:?}");
    assert_ne!(after_drag, after_wheel, "the wheel did not scroll Settings");

    window.wheel(from, 30.0);
    let after_up = first_title();
    println!("wheel up: first item {after_wheel:?} -> {after_up:?}");
    assert_ne!(after_wheel, after_up, "the wheel did not scroll back up");

    device.shell("input keyevent KEYCODE_HOME");
    window.close();
}

#[test]
#[ignore = "needs a real phone"]
fn whole_phone_follows_rotation() {
    let device = Device::connect();
    let auto = device
        .shell("settings get system accelerometer_rotation")
        .trim()
        .to_string();
    let user = device
        .shell("settings get system user_rotation")
        .trim()
        .to_string();
    device.open_calculator();
    let (ctx, core) = new_core("rotate");
    let mut window = Window::open(&ctx, &core, &device.serial, None);
    let portrait = window.phone.video.size();

    device
        .shell("settings put system accelerometer_rotation 0; settings put system user_rotation 1");
    let started = Instant::now();
    let mut turned = None;
    while started.elapsed() < WAIT {
        window.idle(Duration::from_millis(200));
        let size = window.phone.video.size();
        let rect = window.phone.picture_rect().unwrap();
        if size[0] > size[1] && rect.width() > rect.height() {
            turned = Some((size, rect));
            break;
        }
    }
    device.shell(&format!("settings put system user_rotation {user}; settings put system accelerometer_rotation {auto}"));
    let (landscape, rect) = turned.expect("the window did not turn with the phone");
    println!(
        "rotation: video {portrait:?} -> {landscape:?}, picture {:.0}x{:.0}",
        rect.width(),
        rect.height()
    );

    // Input must land on the turned picture too.
    window.idle(Duration::from_secs(1));
    let seven = device.seven();
    if let Some(seven) = seven {
        device.shell("input keyevent KEYCODE_DEL");
        let before = device.formula();
        window.click(PointerButton::Primary, window.at(&device, seven));
        thread::sleep(Duration::from_millis(800));
        println!(
            "tap after rotating: formula {before:?} -> {:?}",
            device.formula()
        );
    }
    let started = Instant::now();
    while window.phone.video.size()[0] > window.phone.video.size()[1] && started.elapsed() < WAIT {
        window.idle(Duration::from_millis(200));
    }
    println!("rotated back: video {:?}", window.phone.video.size());
    device.shell("input keyevent KEYCODE_HOME");
    window.close();
}

#[test]
#[ignore = "needs a real phone"]
fn reconnects_after_the_phone_drops_off() {
    let device = Device::connect();
    let (ctx, core) = new_core("reconnect");
    let mut window = Window::open(&ctx, &core, &device.serial, Some(device.calculator.package));
    let frames = window.phone.video.frames();
    // Drops the USB connection from the computer's side, as unplugging would.
    device
        .adb
        .run(
            &["-s", &device.serial, "reconnect"],
            Duration::from_secs(10),
        )
        .unwrap();
    let started = Instant::now();
    let mut dropped = false;
    while started.elapsed() < Duration::from_secs(40) {
        window.idle(Duration::from_millis(250));
        let (status, live) = window.phone.status_text();
        if !live {
            if !dropped {
                println!("after {:?}: {status}", started.elapsed());
            }
            dropped = true;
        } else if dropped && window.phone.video.frames() > 0 {
            break;
        }
    }
    assert!(dropped, "the window never noticed the phone dropping off");
    assert!(
        window.phone.status_text().1,
        "the window did not reconnect: {:?}",
        window.phone.status_text()
    );
    println!(
        "reconnected by itself after {:?} (frames before {frames})",
        started.elapsed()
    );
    window.close();
}

#[test]
#[ignore = "needs a real phone"]
fn dropping_files_sends_them_and_installs_apks() {
    let device = Device::connect();
    let (ctx, core) = new_core("drop");
    let mut window = Window::open(&ctx, &core, &device.serial, Some(device.calculator.package));

    let folder = std::env::temp_dir().join(format!("openvysor-drop-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("openvysor drop test.txt");
    std::fs::write(&file, "dropped on a phone window").unwrap();
    window.frame(Vec::new(), vec![file.clone()]);
    let remote = "/sdcard/Download/openvysor drop test.txt";
    device.wait_for("the dropped file on the phone", || {
        device
            .shell(&format!("cat '{remote}' 2>/dev/null"))
            .contains("dropped on a phone window")
    });
    window.idle(Duration::from_millis(500));
    println!("file: {:?}", window.phone.notice());
    device.shell(&format!("rm -f '{remote}'"));

    // Reinstalls the phone's own copy of a small app, which changes nothing but the install time.
    let package = small_single_apk_package(&device).expect("no small single-APK app to reinstall");
    let apk_path = device
        .shell(&format!("pm path {package}"))
        .trim()
        .trim_start_matches("package:")
        .to_string();
    let apk = folder.join(format!("{package}.apk"));
    device
        .adb
        .run(
            &[
                "-s",
                &device.serial,
                "pull",
                &apk_path,
                &apk.to_string_lossy(),
            ],
            Duration::from_secs(60),
        )
        .unwrap();
    let installed_at = || {
        device
            .shell(&format!(
                "dumpsys package {package} | grep -m1 lastUpdateTime"
            ))
            .trim()
            .to_string()
    };
    let before = installed_at();
    window.frame(Vec::new(), vec![apk.clone()]);
    device.wait_for("the dropped APK to install", || installed_at() != before);
    window.idle(Duration::from_millis(800));
    println!(
        "apk {package}: {before} -> {}; notice {:?}",
        installed_at(),
        window.phone.notice()
    );
    let _ = std::fs::remove_dir_all(&folder);
    window.close();
}

fn small_single_apk_package(device: &Device) -> Option<String> {
    // One shell call: each third-party app with a single APK, as "size package".
    let script = r#"for p in $(pm list packages -3 | cut -d: -f2); do
        paths=$(pm path $p)
        [ $(echo "$paths" | wc -l) -eq 1 ] && echo $(stat -c %s ${paths#package:}) $p
    done"#;
    let listing = device
        .adb
        .device_text(&device.serial, &["shell", script], Duration::from_secs(120))
        .unwrap_or_default();
    let mut candidates: Vec<(u64, String)> = listing
        .lines()
        .filter_map(|line| {
            let (size, package) = line.trim().split_once(' ')?;
            Some((size.parse().ok()?, package.to_string()))
        })
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .find(|(size, _)| *size < 30_000_000)
        .map(|(_, package)| package)
}

#[test]
#[ignore = "needs a real phone"]
fn sound_button_streams_phone_audio() {
    let device = Device::connect();
    let (ctx, core) = new_core("sound");
    let mut window = Window::open(&ctx, &core, &device.serial, None);
    assert_eq!(
        window.phone.video.audio_bytes(),
        0,
        "sound arrived before it was asked for"
    );
    window.press_toolbar("Play phone sound here (the phone goes silent)");
    window.wait_until_live();
    device.wait_for("phone sound to arrive", || {
        window.frame(Vec::new(), Vec::new());
        window.phone.video.audio_bytes() > 0
    });
    println!(
        "sound: {} bytes so far, loudest {}",
        window.phone.video.audio_bytes(),
        window.phone.video.loudest_sample()
    );

    // Android shares only media and game sound, so play a short 440 Hz tone in the phone's own player.
    let tone = "/sdcard/Download/openvysor-tone.mp4";
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone.mp4");
    device
        .adb
        .run(
            &[
                "-s",
                &device.serial,
                "push",
                &fixture.to_string_lossy(),
                tone,
            ],
            Duration::from_secs(30),
        )
        .unwrap();
    let player = PLAYERS
        .iter()
        .find(|player| {
            device
                .shell(&format!("pm path {}", player.split('/').next().unwrap()))
                .contains("package:")
        })
        .expect("no known video player on this phone");
    let quiet = window.phone.video.loudest_sample();
    device.shell(&format!("am start -n {player} -a android.intent.action.VIEW -d file://{tone} -t video/mp4 >/dev/null 2>&1"));
    let started = Instant::now();
    while window.phone.video.loudest_sample() < 3000 && started.elapsed() < Duration::from_secs(10)
    {
        window.idle(Duration::from_millis(200));
    }
    let loudest = window.phone.video.loudest_sample();
    device.shell(&format!(
        "am force-stop {}; rm -f {tone}; input keyevent KEYCODE_HOME",
        player.split('/').next().unwrap()
    ));
    println!(
        "tone in {player}: {} bytes, loudest {loudest} (was {quiet})",
        window.phone.video.audio_bytes()
    );
    assert!(
        loudest >= 3000,
        "the tone never reached the computer; loudest sample {loudest}"
    );
    window.close();
}

#[test]
#[ignore = "needs a real phone"]
fn per_app_quality_and_position_apply() {
    let device = Device::connect();
    let (ctx, core) = new_core("quality");
    core.update_profile(Some(device.calculator.package), |profile| {
        profile.quality = Some(Quality::Smooth);
        profile.position = Some([123.0, 77.0]);
    });
    let window = Window::open(&ctx, &core, &device.serial, Some(device.calculator.package));
    let smooth = window.phone.video.size();
    let position = window.phone.viewport_builder().position;
    println!("Smooth: video {smooth:?}, window placed at {position:?}");
    assert_eq!(
        smooth.into_iter().max(),
        Some(1280),
        "Smooth should send at most 1280 pixels"
    );
    assert_eq!(position, Some(pos2(123.0, 77.0)));
    window.close();

    core.update_profile(Some(device.calculator.package), |profile| {
        profile.quality = Some(Quality::Sharp)
    });
    let window = Window::open(&ctx, &core, &device.serial, Some(device.calculator.package));
    let sharp = window.phone.video.size();
    println!("Sharp: video {sharp:?}");
    assert!(
        sharp.into_iter().max() > smooth.into_iter().max(),
        "Sharp should be larger than Smooth"
    );
    window.close();
}

#[test]
#[ignore = "needs a real phone"]
fn workspace_reopens_its_windows() {
    let device = Device::connect();
    let (ctx, core) = new_core("workspace");
    let first = Window::open(&ctx, &core, &device.serial, Some(device.calculator.package));
    let second = Window::open(&ctx, &core, &device.serial, Some(device.clock));
    core.save_workspace("Test");
    first.close();
    second.close();
    core.phones.lock().unwrap().clear();

    let workspace = core
        .settings
        .lock()
        .unwrap()
        .workspaces
        .iter()
        .find(|w| w.name == "Test")
        .cloned()
        .unwrap();
    println!(
        "saved {} windows: {:?}",
        workspace.windows.len(),
        workspace
            .windows
            .iter()
            .map(|w| &w.package)
            .collect::<Vec<_>>()
    );
    core.open_workspace(&ctx, &workspace);
    let phones = core.phones();
    assert_eq!(phones.len(), 2, "the workspace did not open both windows");
    for phone in phones {
        let mut window = Window {
            ctx: ctx.clone(),
            core: core.clone(),
            phone,
            started: Instant::now(),
            buttons: Vec::new(),
            commands: Vec::new(),
        };
        window.wait_until_live();
        println!(
            "reopened {}: {:?}, video {:?}",
            window.phone.title(),
            window.phone.status_text(),
            window.phone.video.size()
        );
        window.close();
    }
}

#[test]
#[ignore = "needs a real phone"]
fn mapped_key_taps_its_spot() {
    let device = Device::connect();
    device.open_calculator();
    let seven = device.seven().unwrap();
    let (width, height) = device.display_size();
    let (ctx, core) = new_core("keys");
    core.update_profile(None, |profile| {
        profile.keys = vec![KeyBinding {
            key: "G".to_string(),
            x: seven.0 / width,
            y: seven.1 / height,
        }];
    });
    let mut window = Window::open(&ctx, &core, &device.serial, None);
    window.press_toolbar("Use mapped keys");
    window.key(Key::G);
    window.key(Key::G);
    device.wait_for("the mapped key to tap 7 twice", || {
        device.formula().contains("77")
    });
    println!("mapped key G: formula is {:?}", device.formula());
    device.shell("input keyevent KEYCODE_HOME");
    window.close();
}

#[test]
#[ignore = "needs a real phone"]
fn file_browser_uploads_and_downloads() {
    let device = Device::connect();
    let (ctx, core) = new_core("files");
    let mut files = FilesPage::default();
    let serial = device.serial.clone();
    let mut show = |ui: &mut Ui| files.show(ui, &core, &serial);
    let mut page = Page::new(&ctx);

    page.click("Download", &mut show);
    let name = format!("openvysor upload test {}.txt", std::process::id());
    let folder = std::env::temp_dir().join(format!("openvysor-files-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let local = folder.join(&name);
    std::fs::write(&local, "uploaded from the file browser").unwrap();
    page.frame(Vec::new(), vec![local.clone()], &mut show);
    // The folder may be long, so narrow it to the new file with the filter box.
    page.click("text field", &mut show);
    page.frame(vec![Event::Text(name.clone())], Vec::new(), &mut show);
    page.wait_for(&format!("Download {name}"), &mut show);
    let remote = format!("/sdcard/Download/{name}");
    assert_eq!(
        device.shell(&format!("cat '{remote}'")),
        "uploaded from the file browser"
    );
    println!("uploaded {name} to Download, and the folder shows it");

    page.click(&format!("Download {name}"), &mut show);
    let saved = dirs::download_dir().unwrap().join("OpenVysor").join(&name);
    let started = Instant::now();
    while std::fs::read_to_string(&saved).ok().as_deref() != Some("uploaded from the file browser")
    {
        assert!(
            started.elapsed() < WAIT,
            "the download never reached {}",
            saved.display()
        );
        page.frame(Vec::new(), Vec::new(), &mut show);
        thread::sleep(Duration::from_millis(100));
    }
    println!("downloaded it back to {}", saved.display());

    device.shell(&format!("rm -f '{remote}'"));
    let _ = std::fs::remove_file(&saved);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
#[ignore = "needs a real phone"]
fn notifications_page_shows_a_new_notification() {
    let device = Device::connect();
    let (ctx, core) = new_core("notifications");
    let notifications = NotificationsPage::default();
    let serial = device.serial.clone();
    let mut show = |ui: &mut Ui| notifications.show(ui, &core, &serial);
    let mut page = Page::new(&ctx);

    // Off until the person turns it on.
    page.click("Turn on", &mut show);
    assert!(core.settings.lock().unwrap().notifications);
    // Only this test's own notification is looked for; the phone's others are never printed.
    device.shell("cmd notification post -t 'OpenVysor test' openvysor-page 'Shown on the Notifications page' >/dev/null");
    page.wait_for("Open OpenVysor test", &mut show);
    // Some phones have no `cancel`; stopping Shell then clears the notifications it posted.
    device.shell("cmd notification cancel openvysor-page 2>/dev/null; cmd notification list | grep -q openvysor-page && am force-stop com.android.shell");
    println!("the page shows the new notification");
    notifications.clear();
}

#[test]
#[ignore = "needs a real phone"]
fn alt_arrows_move_between_phone_windows() {
    let device = Device::connect();
    let (ctx, core) = new_core("alt-arrows");
    let mut first = Window::open(&ctx, &core, &device.serial, Some(device.calculator.package));
    let second = Window::open(&ctx, &core, &device.serial, Some(device.clock));
    let focused = |window: &mut Window, key: Key| {
        let alt = Modifiers::ALT;
        let at = window.phone.picture_rect().unwrap().center();
        let events = vec![
            Event::PointerMoved(at),
            Event::Key {
                key,
                physical_key: Some(key),
                pressed: true,
                repeat: false,
                modifiers: alt,
            },
            Event::Key {
                key,
                physical_key: Some(key),
                pressed: false,
                repeat: false,
                modifiers: alt,
            },
        ];
        window.frame(events, Vec::new());
        window
            .commands
            .iter()
            .filter(|(_, command)| *command == ViewportCommand::Focus)
            .map(|(id, _)| *id)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        focused(&mut first, Key::ArrowRight),
        [second.phone.id],
        "Alt+Right did not bring the next window forward"
    );
    assert_eq!(
        focused(&mut first, Key::ArrowLeft),
        [second.phone.id],
        "Alt+Left did not wrap around to the last window"
    );
    println!("Alt+Right and Alt+Left from the first window both focus the second");
    first.close();
    second.close();
}
