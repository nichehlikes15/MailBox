//! Tokio lives on its own runtime, separate from gpui.
//!
//! gpui has its own executors but no IO reactor, so anything that talks to the
//! network (reqwest, axum, eventsource) runs on this runtime. UI code awaits
//! the result through [`spawn`] and then applies it on the gpui side.

use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::task::{JoinError, JoinHandle};

// `OnceLock` = a global that's created the first time it's used and then
// reused forever. Both the runtime and the HTTP client are built once.
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static HTTP: OnceLock<reqwest::Client> = OnceLock::new();

pub fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            // Two threads is plenty: this app is waiting on the network, not crunching
            // numbers. (Zed itself uses a single worker thread for the same job.)
            .worker_threads(2)
            .thread_name("mailbox-io")
            .enable_all()
            .build()
            .expect("Failed to start tokio runtime")
    })
}

/// One shared HTTP client so connections (and TLS sessions) get reused
/// instead of doing a fresh handshake on every request.
// Why share one client? Creating a `reqwest::Client` per request (what the
// old code did) means a brand new TCP connection + TLS handshake every time,
// which is slow and CPU-heavy. A shared client keeps connections open and
// reuses them. `Client` is cheap to share: internally it's an `Arc`.
pub fn http() -> &'static reqwest::Client {
    HTTP.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client")
    })
}

/// Client for long-lived streams (SSE). No overall timeout, or the stream
/// would be cut off after 30 seconds.
pub fn http_streaming() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("Failed to build streaming HTTP client")
}

/// Run a future on the tokio runtime. The returned handle aborts the tokio
/// task when dropped, so dropping the gpui `Task` that awaits it cancels the
/// network work too.
// Example of how UI code uses this:
//
//     let io = crate::runtime::spawn(async move { fetch_something().await });
//     cx.spawn(async move |this, cx| {
//         let result = io.await;            // waits without blocking the UI
//         this.update(cx, |view, cx| { ... apply result ... });
//     });
//
// The work runs on tokio's threads; only the final `update` touches the UI.
// The `Send + 'static` bounds are required because the future moves to
// another thread: it can't borrow anything from the caller and everything
// inside it must be safe to send between threads.
pub fn spawn<F>(future: F) -> AbortOnDrop<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    AbortOnDrop(runtime().spawn(future))
}

// Wrapper around tokio's `JoinHandle`. A plain `JoinHandle` does NOT stop the
// task when dropped (the task keeps running in the background). We want the
// opposite: when the UI no longer cares about a request (user clicked a
// different email, switched account, closed the view), dropping the handle
// should cancel the network work too. `abort()` in `Drop` does exactly that.
pub struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

// Lets you `.await` an `AbortOnDrop` directly. It just forwards to the inner
// `JoinHandle`. The output is `Result<T, JoinError>`: `Err` means the task
// panicked or was aborted.
impl<T> Future for AbortOnDrop<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}
