use std::process::{Child, Command};

use crate::error::Result;

#[cfg(windows)]
pub use windows_job::JobObject;

#[cfg(unix)]
pub use unix_group::JobObject;

#[cfg(windows)]
mod windows_job {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    use super::*;

    /// Processes assigned to this job die when the app exits, even after a crash.
    /// The adb server must stay out of it because other tools may share that server.
    pub struct JobObject(HANDLE);

    // A job handle is an opaque kernel handle that may be used from any thread.
    unsafe impl Send for JobObject {}
    unsafe impl Sync for JobObject {}

    impl JobObject {
        pub fn new() -> Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(std::io::Error::last_os_error().into());
                }
                let job = Self(handle);
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&raw const info).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                Ok(job)
            }
        }

        pub fn spawn(&self, command: &mut Command) -> Result<Child> {
            let child = command.spawn()?;
            let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) };
            if ok == 0 {
                log::warn!("could not assign the process to the job: {}", std::io::Error::last_os_error());
            }
            Ok(child)
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(unix)]
mod unix_group {
    use std::os::unix::process::CommandExt;
    use std::process::{ChildStdin, Stdio};

    use super::*;

    /// Waits until its input closes, which also happens when the app crashes,
    /// and then kills its whole process group, itself included.
    const WATCHER: &str = "cat >/dev/null; kill -KILL 0";

    /// Processes started through this die when the app exits, even after a crash.
    /// They share a process group led by a small watcher that holds the group id.
    pub struct JobObject {
        watcher: Child,
        pipe: Option<ChildStdin>,
    }

    impl JobObject {
        pub fn new() -> Result<Self> {
            let mut watcher = Command::new("/bin/sh")
                .args(["-c", WATCHER])
                .process_group(0)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            let pipe = watcher.stdin.take();
            Ok(Self { watcher, pipe })
        }

        pub fn spawn(&self, command: &mut Command) -> Result<Child> {
            Ok(command.process_group(self.watcher.id() as i32).spawn()?)
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            self.pipe.take();
            let _ = self.watcher.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    fn long_running() -> Command {
        let mut command = if cfg!(windows) { Command::new("ping") } else { Command::new("sleep") };
        if cfg!(windows) {
            command.args(["-n", "30", "127.0.0.1"]);
        } else {
            command.arg("30");
        }
        command.stdout(Stdio::null());
        command
    }

    #[test]
    fn closing_the_job_kills_its_processes() {
        let job = JobObject::new().unwrap();
        let mut child = job.spawn(&mut long_running()).unwrap();
        assert!(child.try_wait().unwrap().is_none());

        drop(job);

        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "child outlived its job");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
