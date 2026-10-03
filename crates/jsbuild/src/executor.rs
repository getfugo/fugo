//! Runs rolldown's async builds for synchronous callers.
//!
//! A build is a task on a process-wide tokio runtime; the caller blocks on a channel until it
//! is done. The caller must not do anything else while it waits: this port realizes a resource
//! under that resource's lock, and a waiting thread that ran other work (such as
//! `rayon::yield_now`) could take the same lock again and deadlock.
//!
//! rolldown parallelizes on the global rayon pool and waits for it inside a build. A caller on
//! a global-pool worker therefore takes a thread the build may need; this port calls from its own
//! render pool, never from the global one.

use std::future::Future;
use std::sync::LazyLock;
use std::sync::mpsc;

use tokio::runtime::{Builder, Runtime};

/// The runtime, or why it could not be started.
static RUNTIME: LazyLock<Result<Runtime, String>> = LazyLock::new(|| {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    Builder::new_multi_thread()
        .worker_threads(threads)
        .thread_name("ssg-jsbuild")
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
});

/// Runs `build` to completion. `Err` when the runtime could not start or the build panicked.
pub(crate) fn run<T: Send + 'static>(
    build: impl Future<Output = T> + Send + 'static,
) -> Result<T, String> {
    let runtime = RUNTIME
        .as_ref()
        .map_err(|e| format!("cannot start the bundler runtime: {e}"))?;
    let (tx, rx) = mpsc::sync_channel(1);
    runtime.spawn(async move {
        // The receiver only goes away with the caller.
        let _ = tx.send(build.await);
    });
    // A panic in the task drops the sender (tokio keeps the panic in the task).
    rx.recv().map_err(|_| "the bundler panicked".to_owned())
}
