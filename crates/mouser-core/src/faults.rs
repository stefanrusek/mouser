//! The fault boundary (research V3): a unit of work that panics is caught, the
//! fault is reported as an [`EngineEvent::Fault`], and the loop that ran it
//! carries on. The process panic hook writes every panic to the diagnostic
//! log, naming the unit of work it happened in.

use std::cell::Cell;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::task::Poll;

use tokio::sync::broadcast;

use crate::engine::EngineEvent;
use crate::logging::{Log, events};

pub use events::RecoveryAction;

thread_local! {
    /// The unit of work running on this thread, for the panic hook to name.
    static UNIT: Cell<Option<&'static str>> = const { Cell::new(None) };
}

/// Names `at` as this thread's unit of work until dropped.
struct InUnit(Option<&'static str>);

impl InUnit {
    fn enter(at: &'static str) -> Self {
        Self(UNIT.with(|unit| unit.replace(Some(at))))
    }
}

impl Drop for InUnit {
    fn drop(&mut self) {
        UNIT.with(|unit| unit.set(self.0));
    }
}

/// Write every panic of this process to `log` (while logging is on) as a
/// `fault` record naming the unit of work or thread, where it happened and
/// what it said, and then let the hook that was installed before run.
pub fn install(log: Arc<Log>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = UNIT.with(Cell::get).map_or_else(
            || {
                std::thread::current()
                    .name()
                    .unwrap_or("<unnamed>")
                    .to_string()
            },
            str::to_string,
        );
        let place = info.location().expect("a panic has a location");
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string payload");
        log.write(events::fault(
            &at,
            &format!("{}:{}", place.file(), place.line()),
            message,
        ));
        previous(info);
    }));
}

/// A fault at `at` was recovered from by `action`.
pub(crate) fn recovered(log: &Log, at: &str, action: RecoveryAction) {
    log.write(events::fault_recovered(at, action));
}

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
    at: &'static str,
    f: impl FnOnce() -> T,
) -> Result<T, Faulted> {
    let _in = InUnit::enter(at);
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| report(events, at))
}

/// [`run_unit`] for a unit that awaits: a panic in any poll of `unit` is a
/// fault at `at`.
pub(crate) async fn run_unit_async<T>(
    events: &broadcast::Sender<EngineEvent>,
    at: &'static str,
    unit: impl Future<Output = T>,
) -> Result<T, Faulted> {
    let mut unit = std::pin::pin!(unit);
    std::future::poll_fn(|cx| {
        let _in = InUnit::enter(at);
        match catch_unwind(AssertUnwindSafe(|| unit.as_mut().poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Err(_) => Poll::Ready(Err(report(events, at))),
        }
    })
    .await
}
