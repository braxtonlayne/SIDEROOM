//! SEC-AUD-029: one panic in a long-lived loop must not end the loop.
//!
//! The event consumers and tick loops spawned at startup run their handler inline; a panic would unwind out of the
//! loop and end the task silently (mirroring, push, search indexing, the staff feed). [`step`] runs one handler call
//! and turns a panic into an error log line, so the loop goes on with the next event or tick.

use futures::FutureExt;
use std::{future::Future, panic::AssertUnwindSafe};

/// Run `fut` (one event or one tick of the loop named `what`), logging a panic instead of propagating it. Returns
/// `false` when it panicked.
pub async fn step<F: Future<Output = ()>>(what: &'static str, fut: F) -> bool {
    match AssertUnwindSafe(fut).catch_unwind().await {
        Ok(()) => true,
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "non-string panic".to_owned());
            tracing::error!(task = what, panic = %msg, "a background step panicked; the loop goes on");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn a_panicking_event_does_not_end_the_loop() {
        let mut events = futures::stream::iter([1, 2, 3]);
        let mut handled = Vec::new();
        let mut panics = 0;
        while let Some(n) = events.next().await {
            let ok = step("test loop", async {
                if n == 1 {
                    panic!("boom on {n}");
                }
                handled.push(n);
            })
            .await;
            if !ok {
                panics += 1;
            }
        }
        assert_eq!(handled, vec![2, 3]);
        assert_eq!(panics, 1);
    }
}
