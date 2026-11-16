//! Running `onlydiffs-core` from inside GPUI.
//!
//! The core is written against tokio — child processes, sockets, timers — and
//! GPUI drives its own executor. Rather than make either adopt the other, the
//! core's futures run on a small tokio runtime of their own, and GPUI awaits
//! the `JoinHandle`, which is an ordinary future that any executor can poll.

use std::future::Future;
use std::sync::OnceLock;

use anyhow::{Context as _, Result};
use tokio::runtime::Runtime;

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("onlydiffs-core")
            .enable_all()
            .build()
            .expect("the tokio runtime starts")
    })
}

/// Runs `future` on the core's runtime and resolves with its output.
pub async fn run<F, T>(future: F) -> Result<T>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    runtime()
        .spawn(future)
        .await
        .context("a background task stopped before it finished")
}
