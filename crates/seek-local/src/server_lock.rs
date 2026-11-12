//! The lock of the tests that start a llama server.
//!
//! The tests of the workspace run in parallel processes, and every server takes a couple of
//! gigabytes: three of them at once take the machine down, and the tests never end. The lock
//! is a file, so it works between the processes as well.

use std::fs::File;

/// The name of the lock file. It is the same for every crate, on purpose.
const LOCK_NAME: &str = "seek-llama-server.lock";

/// How long a test waits for the lock before it gives up.
const WAIT: std::time::Duration = std::time::Duration::from_secs(600);

/// The lock of the server. It is released when it is dropped.
pub struct ServerLock {
    /// The lock itself. It is released when it is dropped.
    _lock: nix::fcntl::Flock<File>,
}

impl ServerLock {
    /// Take the lock, waiting for the other tests to give it back.
    ///
    /// The wait is bounded: a test that cannot get the lock fails instead of hanging for
    /// ever.
    pub fn take() -> Self {
        let path = std::env::temp_dir().join(LOCK_NAME);
        let deadline = std::time::Instant::now() + WAIT;
        loop {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&path)
                .expect("the lock file can be opened");
            let lock = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock);
            match lock {
                Ok(lock) => return ServerLock { _lock: lock },
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                Err(_) => panic!("the lock of the llama server is taken: {path:?}"),
            }
        }
    }
}
