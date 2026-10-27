//! The fault boundary (research V3): a unit of work that panics is caught, the
//! fault is reported as an [`EngineEvent::Fault`], and the loop that ran it
//! carries on.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::task::Poll;

use tokio::sync::broadcast;

use crate::engine::EngineEvent;

/// The unit of work panicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Faulted;

fn report(events: &broadcast::Sender<EngineEvent>, at: &str) -> Faulted {
    // No subscribers is fine.
    let _ = events.send(EngineEvent::Fault {
        r#where: at.to_string(),
    });
    Faulted
}

/// Run `f`; a panic in it is a fault at `at`, not the end of the caller.
pub(crate) fn run_unit<T>(
    events: &broadcast::Sender<EngineEvent>,
    at: &str,
    f: impl FnOnce() -> T,
) -> Result<T, Faulted> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| report(events, at))
}

/// [`run_unit`] for a unit that awaits: a panic in any poll of `unit` is a
/// fault at `at`.
pub(crate) async fn run_unit_async<T>(
    events: &broadcast::Sender<EngineEvent>,
    at: &str,
    unit: impl Future<Output = T>,
) -> Result<T, Faulted> {
    let mut unit = std::pin::pin!(unit);
    std::future::poll_fn(
        |cx| match catch_unwind(AssertUnwindSafe(|| unit.as_mut().poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Err(_) => Poll::Ready(Err(report(events, at))),
        },
    )
    .await
}
