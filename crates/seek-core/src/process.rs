//! The processes SEEK started (SPEC 9.1).
//!
//! The `bash` tool runs every command in its own process group and registers it, so the sidebar
//! can show it and the user can kill it.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The tag of a process the agent started.
pub const TAG_AGENT: &str = "agent";
/// The tag of the local model server.
pub const TAG_SERVER: &str = "server";

/// One running process.
#[derive(Debug, Clone)]
pub struct ProcessEntry {
    pub pid: u32,
    /// `agent` or `server`.
    pub tag: String,
    /// The command line.
    pub command: String,
    /// When it started.
    pub started: Instant,
}

impl ProcessEntry {
    /// How long the process has been running.
    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    /// The uptime as `1h 02m` or `12s`.
    pub fn uptime_text(&self) -> String {
        let seconds = self.uptime().as_secs();
        if seconds >= 3600 {
            format!("{}h {:02}m", seconds / 3600, (seconds % 3600) / 60)
        } else if seconds >= 60 {
            format!("{}m {:02}s", seconds / 60, seconds % 60)
        } else {
            format!("{seconds}s")
        }
    }
}

/// The processes that are running.
#[derive(Debug, Default)]
pub struct ProcessRegistry {
    processes: Mutex<Vec<ProcessEntry>>,
}

impl ProcessRegistry {
    pub fn new() -> Self {
        ProcessRegistry::default()
    }

    /// Register a process.
    pub fn register(&self, pid: u32, tag: &str, command: &str) {
        let mut processes = self.processes.lock().expect("the registry is not poisoned");
        processes.retain(|entry| entry.pid != pid);
        processes.push(ProcessEntry {
            pid,
            tag: tag.to_string(),
            command: command.to_string(),
            started: Instant::now(),
        });
    }

    /// Forget a process.
    pub fn remove(&self, pid: u32) {
        let mut processes = self.processes.lock().expect("the registry is not poisoned");
        processes.retain(|entry| entry.pid != pid);
    }

    /// The running processes, oldest first.
    pub fn list(&self) -> Vec<ProcessEntry> {
        self.processes
            .lock()
            .expect("the registry is not poisoned")
            .clone()
    }

    /// Whether a process is registered.
    pub fn contains(&self, pid: u32) -> bool {
        self.processes
            .lock()
            .expect("the registry is not poisoned")
            .iter()
            .any(|entry| entry.pid == pid)
    }

    /// Kill a process group.
    ///
    /// The signal goes to the whole group, so a shell and its children die together.
    pub fn kill(&self, pid: u32) -> bool {
        let killed = kill_group(pid);
        if killed {
            self.remove(pid);
        }
        killed
    }
}

/// Send SIGTERM to a process group, then SIGKILL if it is still there.
fn kill_group(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;

        let pid = Pid::from_raw(pid as i32);
        if killpg(pid, Signal::SIGTERM).is_err() {
            return false;
        }
        // Give the group a moment to shut down, then insist.
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(20));
            if killpg(pid, Signal::SIGKILL).is_err() {
                return true;
            }
        }
        let _ = killpg(pid, Signal::SIGKILL);
        true
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_is_registered_and_removed() {
        let registry = ProcessRegistry::new();
        registry.register(1234, TAG_AGENT, "cargo test");
        assert!(registry.contains(1234));
        assert_eq!(registry.list().len(), 1);
        assert_eq!(registry.list()[0].command, "cargo test");

        registry.remove(1234);
        assert!(!registry.contains(1234));
    }

    #[test]
    fn registering_the_same_pid_twice_keeps_one_entry() {
        let registry = ProcessRegistry::new();
        registry.register(1234, TAG_AGENT, "one");
        registry.register(1234, TAG_AGENT, "two");
        assert_eq!(registry.list().len(), 1);
        assert_eq!(registry.list()[0].command, "two");
    }

    #[test]
    fn the_uptime_is_readable() {
        let entry = ProcessEntry {
            pid: 1,
            tag: TAG_AGENT.into(),
            command: "sleep 1".into(),
            started: Instant::now(),
        };
        assert!(entry.uptime_text().ends_with('s'));
    }
}
