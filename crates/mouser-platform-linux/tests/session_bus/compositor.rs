//! A minimal compositor that implements `ext-data-control` on one seat: it
//! relays selections between the clients that connect to it.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use wayland_protocols::ext::data_control::v1::server::ext_data_control_device_v1::{
    self, ExtDataControlDeviceV1,
};
use wayland_protocols::ext::data_control::v1::server::ext_data_control_manager_v1::{
    self, ExtDataControlManagerV1,
};
use wayland_protocols::ext::data_control::v1::server::ext_data_control_offer_v1::{
    self, ExtDataControlOfferV1,
};
use wayland_protocols::ext::data_control::v1::server::ext_data_control_source_v1::{
    self, ExtDataControlSourceV1,
};
use wayland_server::protocol::wl_seat::WlSeat;
use wayland_server::{
    Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, ListeningSocket, New,
    Resource,
};

#[derive(Default)]
struct State {
    devices: Vec<ExtDataControlDeviceV1>,
    types: HashMap<ExtDataControlSourceV1, Vec<String>>,
    selection: Option<ExtDataControlSourceV1>,
}

impl GlobalDispatch<WlSeat, ()> for State {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        seat: New<WlSeat>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(seat, ()).name("seat0".into());
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &WlSeat,
        _: <WlSeat as Resource>::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}

impl GlobalDispatch<ExtDataControlManagerV1, ()> for State {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        manager: New<ExtDataControlManagerV1>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        init.init(manager, ());
    }
}

/// Tells `device` what the selection is now.
fn announce(state: &State, handle: &DisplayHandle, device: &ExtDataControlDeviceV1) {
    // A device whose client has gone has nobody to tell.
    let Some(client) = device.client() else {
        return;
    };
    match &state.selection {
        None => device.selection(None),
        Some(source) => {
            let offer = client
                .create_resource::<ExtDataControlOfferV1, _, State>(handle, device.version(), ())
                .unwrap();
            device.data_offer(&offer);
            for mime in &state.types[source] {
                offer.offer(mime.clone());
            }
            device.selection(Some(&offer));
        }
    }
}

impl Dispatch<ExtDataControlManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &ExtDataControlManagerV1,
        request: ext_data_control_manager_v1::Request,
        _: &(),
        handle: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ext_data_control_manager_v1::Request::CreateDataSource { id } => {
                let source = init.init(id, ());
                state.types.insert(source, Vec::new());
            }
            ext_data_control_manager_v1::Request::GetDataDevice { id, .. } => {
                let device = init.init(id, ());
                announce(state, handle, &device);
                state.devices.push(device);
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlDeviceV1, ()> for State {
    fn request(
        state: &mut Self,
        _: &Client,
        setter: &ExtDataControlDeviceV1,
        request: ext_data_control_device_v1::Request,
        _: &(),
        handle: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        if let ext_data_control_device_v1::Request::SetSelection { source } = request {
            if let Some(old) = state.selection.take()
                && Some(&old) != source.as_ref()
            {
                old.cancelled();
            }
            state.selection = source;
            for device in state.devices.clone() {
                if device != *setter {
                    announce(state, handle, &device);
                }
            }
        }
    }
}

impl Dispatch<ExtDataControlOfferV1, ()> for State {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &ExtDataControlOfferV1,
        request: ext_data_control_offer_v1::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        if let ext_data_control_offer_v1::Request::Receive { mime_type, fd } = request {
            use std::os::fd::AsFd;
            state
                .selection
                .as_ref()
                .unwrap()
                .send(mime_type, fd.as_fd());
        }
    }
}

impl Dispatch<ExtDataControlSourceV1, ()> for State {
    fn request(
        state: &mut Self,
        _: &Client,
        source: &ExtDataControlSourceV1,
        request: ext_data_control_source_v1::Request,
        _: &(),
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        if let ext_data_control_source_v1::Request::Offer { mime_type } = request {
            state.types.get_mut(source).unwrap().push(mime_type);
        }
    }
}

/// The running compositor; dropping it disconnects every client.
pub struct Compositor {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Compositor {
    /// Listens on `socket`, which the caller already points clients at.
    pub fn start(socket: std::path::PathBuf) -> Compositor {
        let mut display = Display::<State>::new().unwrap();
        display.handle().create_global::<State, WlSeat, _>(6, ());
        display
            .handle()
            .create_global::<State, ExtDataControlManagerV1, _>(1, ());
        let listener = ListeningSocket::bind_absolute(socket.clone()).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut state = State::default();
            while !stopping.load(Ordering::SeqCst) {
                if let Some(stream) = listener.accept().unwrap() {
                    display
                        .handle()
                        .insert_client(stream, Arc::new(Clients))
                        .unwrap();
                }
                display.dispatch_clients(&mut state).unwrap();
                display.flush_clients().unwrap();
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        Compositor {
            stop,
            thread: Some(thread),
        }
    }
}

struct Clients;

impl wayland_server::backend::ClientData for Clients {}

impl Drop for Compositor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let joined = self.thread.take().unwrap().join();
        // A second panic while one is unwinding would abort the process.
        if !std::thread::panicking() {
            joined.unwrap();
        }
    }
}
