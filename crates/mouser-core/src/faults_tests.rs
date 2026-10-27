use tokio::sync::broadcast;

use crate::EngineEvent;
use crate::faults::{Faulted, run_unit, run_unit_async};

fn events() -> (
    broadcast::Sender<EngineEvent>,
    broadcast::Receiver<EngineEvent>,
) {
    broadcast::channel(8)
}

#[test]
fn a_unit_that_does_not_panic_returns_its_value() {
    let (tx, mut rx) = events();
    assert_eq!(run_unit(&tx, "reconcile", || 7), Ok(7));
    assert!(rx.try_recv().is_err());
}

#[test]
fn a_unit_that_panics_is_faulted_reported_and_the_caller_continues() {
    let (tx, mut rx) = events();
    let mut ticks = 0;
    for tick in 0..3 {
        let result = run_unit(&tx, "reconcile", || {
            if tick == 1 {
                panic!("scripted fault");
            }
            tick
        });
        ticks += 1;
        assert_eq!(result.is_err(), tick == 1);
    }
    assert_eq!(ticks, 3);
    assert_eq!(
        rx.try_recv().unwrap(),
        EngineEvent::Fault {
            r#where: "reconcile".into()
        }
    );
    assert!(rx.try_recv().is_err());
    assert_eq!(
        run_unit(&tx, "x", || -> u8 { panic!("again") }),
        Err(Faulted)
    );
}

#[tokio::test]
async fn an_async_unit_returns_its_value_across_awaits() {
    let (tx, mut rx) = events();
    let value = run_unit_async(&tx, "control", async {
        tokio::task::yield_now().await;
        9
    })
    .await;
    assert_eq!(value, Ok(9));
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn an_async_unit_that_panics_is_faulted_and_reported() {
    let (tx, mut rx) = events();
    let value = run_unit_async(&tx, "control", async {
        tokio::task::yield_now().await;
        if true {
            panic!("scripted fault");
        }
        1
    })
    .await;
    assert_eq!(value, Err(Faulted));
    assert_eq!(
        rx.try_recv().unwrap(),
        EngineEvent::Fault {
            r#where: "control".into()
        }
    );
}
