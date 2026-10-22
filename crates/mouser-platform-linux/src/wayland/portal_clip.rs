//! The portal Clipboard on the RemoteDesktop session. The portal needs the
//! clipboard requested before the session starts, so this is created with the
//! session, before it is started; both signals are subscribed then, because
//! the desktop may announce the current selection as soon as it starts.
//!
//! The signals are read raw: GNOME 46 sends `SelectionOwnerChanged` but the
//! typed ashpd stream never yields it.

use crate::clipboard::{WAYLAND_TEXT, decode, encode, mime_types};
use ashpd::desktop::Session;
use ashpd::desktop::clipboard::{Clipboard, SetSelectionOptions};
use ashpd::desktop::remote_desktop::RemoteDesktop;
use futures_util::StreamExt;
use mouser_platform::{ClipRep, Error, Result};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;
use zbus::proxy::SignalStream;
use zbus::zvariant::{OwnedFd, OwnedObjectPath, OwnedValue};

/// What the clipboard is known to hold.
#[derive(Default)]
struct State {
    /// The types another application's selection offers.
    offered: Mutex<Vec<String>>,
    /// What this backend put on the clipboard, while that is still the selection.
    held: Mutex<Option<HashMap<String, Vec<u8>>>>,
    changes: AtomicU64,
}

pub(crate) struct PortalClip {
    portal: Arc<Clipboard>,
    session: Arc<Session<RemoteDesktop>>,
    state: Arc<State>,
    tasks: Vec<JoinHandle<()>>,
}

fn os(error: impl std::fmt::Display) -> Error {
    Error::Os(error.to_string())
}

impl PortalClip {
    /// Requests the clipboard for `session` and starts following it; `None`
    /// when the desktop has no clipboard portal.
    pub(crate) async fn request(
        conn: zbus::Connection,
        session: Arc<Session<RemoteDesktop>>,
    ) -> Option<PortalClip> {
        let portal = Arc::new(Clipboard::with_connection(conn).await.ok()?);
        portal.request(&session, Default::default()).await.ok()?;
        let owners = portal.receive_signal("SelectionOwnerChanged").await.ok()?;
        let transfers = portal.receive_signal("SelectionTransfer").await.ok()?;
        let state = Arc::new(State::default());
        let tasks = vec![
            tokio::spawn(follow_owner(owners, state.clone())),
            tokio::spawn(serve_transfers(
                transfers,
                portal.clone(),
                session.clone(),
                state.clone(),
            )),
        ];
        Some(PortalClip {
            portal,
            session,
            state,
            tasks,
        })
    }

    pub(crate) fn change_count(&self) -> u64 {
        self.state.changes.load(Ordering::SeqCst)
    }

    async fn fetch(&self, mime: &str) -> Result<Vec<u8>> {
        let fd = self
            .portal
            .selection_read(&self.session, mime)
            .await
            .map_err(os)?;
        tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
            let mut data = Vec::new();
            std::fs::File::from(std::os::fd::OwnedFd::from(fd)).read_to_end(&mut data)?;
            Ok(data)
        })
        .await
        .unwrap()
        .map_err(os)
    }

    pub(crate) async fn read(&self) -> Result<Vec<ClipRep>> {
        let held = self.state.held.lock().unwrap().clone();
        let items = match held {
            Some(items) => items,
            None => {
                let offered = self.state.offered.lock().unwrap().clone();
                let mut items = HashMap::new();
                for mime in mime_types(WAYLAND_TEXT) {
                    if offered.iter().any(|o| o == mime) {
                        items.insert(mime.to_string(), self.fetch(mime).await?);
                    }
                }
                items
            }
        };
        decode(WAYLAND_TEXT, |mime| {
            Ok::<_, Error>(items.get(mime).cloned())
        })
    }

    pub(crate) async fn write(&self, reps: Vec<ClipRep>) -> Result<()> {
        let items = encode(WAYLAND_TEXT, &reps);
        let mimes: Vec<&str> = items.iter().map(|(m, _)| m.as_str()).collect();
        let options = SetSelectionOptions::default().set_mime_types(&mimes);
        *self.state.held.lock().unwrap() = Some(items.iter().cloned().collect());
        self.portal
            .set_selection(&self.session, options)
            .await
            .map_err(os)?;
        self.state.changes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for PortalClip {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Another application took the clipboard: remember what it offers.
async fn follow_owner(mut owners: SignalStream<'static>, state: Arc<State>) {
    while let Some(message) = owners.next().await {
        let (_, options): (OwnedObjectPath, HashMap<String, OwnedValue>) =
            message.body().deserialize().unwrap();
        let ours = options
            .get("session_is_owner")
            .is_some_and(|v| bool::try_from(v).unwrap());
        if ours {
            continue;
        }
        let types = options
            .get("mime_types")
            .map(|v| Vec::<String>::try_from(v.try_clone().unwrap()).unwrap())
            .unwrap_or_default();
        *state.offered.lock().unwrap() = types;
        *state.held.lock().unwrap() = None;
        state.changes.fetch_add(1, Ordering::SeqCst);
    }
}

/// An application pasted what this backend holds: write it to the pipe the
/// portal hands out.
async fn serve_transfers(
    mut transfers: SignalStream<'static>,
    portal: Arc<Clipboard>,
    session: Arc<Session<RemoteDesktop>>,
    state: Arc<State>,
) {
    while let Some(message) = transfers.next().await {
        let (_, mime, serial): (OwnedObjectPath, String, u32) =
            message.body().deserialize().unwrap();
        let data = state
            .held
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|items| items.get(&mime).cloned());
        let written = match data {
            Some(data) => {
                let fd: OwnedFd = portal.selection_write(&session, serial).await.unwrap();
                tokio::task::spawn_blocking(move || {
                    std::fs::File::from(std::os::fd::OwnedFd::from(fd)).write_all(&data)
                })
                .await
                .unwrap()
                .is_ok()
            }
            None => false,
        };
        portal
            .selection_write_done(&session, serial, written)
            .await
            .unwrap();
    }
}
