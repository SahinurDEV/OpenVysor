pub mod commands;
pub mod connection;
pub mod device;
pub mod setup;
pub mod wireless;

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{AppError, Result};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Builds a `Command` that never flashes a console window on Windows.
pub fn hidden_command(program: &Path) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[derive(Clone, Debug)]
pub struct Adb {
    path: PathBuf,
}

impl Adb {
    pub fn locate() -> Result<Self> {
        candidate_paths()
            .into_iter()
            .find(|path| path.is_file())
            .map(|path| Self { path })
            .ok_or(AppError::AdbNotFound)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn command(&self) -> Command {
        hidden_command(&self.path)
    }

    /// Starts the adb server. Output is discarded because the daemon can inherit
    /// pipe handles and keep them open for as long as it runs.
    pub fn start_server(&self) -> Result<()> {
        let status = self
            .command()
            .arg("start-server")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(AppError::AdbFailed("could not start the adb server".into()))
        }
    }

    pub fn version(&self) -> Result<String> {
        let text = self.run_text(&["version"], DEFAULT_TIMEOUT)?;
        Ok(text.lines().next().unwrap_or_default().trim().to_string())
    }

    pub fn run<S: AsRef<OsStr>>(&self, args: &[S], timeout: Duration) -> Result<Vec<u8>> {
        let label = args
            .iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let mut child = self
            .command()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = drain(child.stdout.take());
        let stderr = drain(child.stderr.take());
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Timeout(label));
            }
            thread::sleep(Duration::from_millis(5));
        };

        let grace = Duration::from_millis(500);
        let out = stdout.recv_timeout(grace).unwrap_or_default();
        let err = stderr.recv_timeout(grace).unwrap_or_default();
        if status.success() {
            Ok(out)
        } else {
            let message = String::from_utf8_lossy(&err).trim().to_string();
            let message = if message.is_empty() {
                String::from_utf8_lossy(&out).trim().to_string()
            } else {
                message
            };
            Err(AppError::AdbFailed(message))
        }
    }

    pub fn run_text<S: AsRef<OsStr>>(&self, args: &[S], timeout: Duration) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.run(args, timeout)?).into_owned())
    }

    pub fn device_text(&self, serial: &str, args: &[&str], timeout: Duration) -> Result<String> {
        let mut full = vec!["-s", serial];
        full.extend_from_slice(args);
        self.run_text(&full, timeout)
    }
}

fn drain<R: Read + Send + 'static>(source: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut source) = source {
            let _ = source.read_to_end(&mut buffer);
        }
        let _ = tx.send(buffer);
    });
    rx
}

fn candidate_paths() -> Vec<PathBuf> {
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("OPENVYSOR_ADB") {
        paths.push(PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path).map(|dir| dir.join(exe)));
    }
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(root) = std::env::var_os(var) {
            paths.push(PathBuf::from(root).join("platform-tools").join(exe));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        paths.push(local.join("Android/Sdk/platform-tools").join(exe));
        // WinGet package installs do not always reach the PATH of GUI apps.
        if let Ok(entries) = std::fs::read_dir(local.join("Microsoft/WinGet/Packages")) {
            for package in entries.flatten() {
                let name = package.file_name().to_string_lossy().to_lowercase();
                if !name.contains("scrcpy") && !name.contains("platformtools") {
                    continue;
                }
                paths.push(package.path().join("platform-tools").join(exe));
                if let Ok(children) = std::fs::read_dir(package.path()) {
                    paths.extend(children.flatten().map(|child| child.path().join(exe)));
                }
            }
        }
    }
    // Apps opened from the desktop do not get the PATH set in a shell profile.
    #[cfg(not(windows))]
    {
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join("Library/Android/sdk/platform-tools").join(exe));
            paths.push(home.join("Android/Sdk/platform-tools").join(exe));
        }
        paths.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(|dir| Path::new(dir).join(exe)));
    }
    if let Some(folder) = setup::tools_folder() {
        paths.push(folder.join(exe));
    }
    paths
}
