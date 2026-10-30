use mouser_platform::Os;

use crate::tray::{Appearance, OPEN_ID, QUIT_ID};
use crate::tray_not_ready::*;

#[test]
fn while_the_service_engine_is_away_the_tray_says_so_and_offers_open_and_quit() {
    let view = view(Os::Windows, Appearance::Dark);
    assert_eq!(view.summary, "Mouser isn't ready yet");
    assert_eq!(view.tooltip, "Mouser is waiting for its service to start");
    let ids: Vec<_> = view.entries.iter().map(|e| e.id).collect();
    assert_eq!(ids, [None, Some(OPEN_ID), Some(QUIT_ID)]);
    assert_eq!(view.entries[0].label, "Mouser isn't ready yet");
}

#[tokio::test]
async fn following_hears_now_and_every_change_until_nobody_can_change_it() {
    let (ready, seen) = tokio::sync::watch::channel(false);
    let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let into = heard.clone();
    let following = tokio::spawn(follow(seen, move |r| into.lock().unwrap().push(r)));
    // Each change is seen before the next is made.
    let settle = |n: usize| {
        let heard = heard.clone();
        async move {
            while heard.lock().unwrap().len() < n {
                tokio::task::yield_now().await;
            }
        }
    };
    settle(1).await;
    ready.send_replace(true);
    settle(2).await;
    ready.send_replace(false);
    settle(3).await;
    drop(ready);
    following.await.unwrap();
    assert_eq!(*heard.lock().unwrap(), [false, true, false]);
}
