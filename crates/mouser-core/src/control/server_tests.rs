use mouser_proto::control::{Desk, DeskCreateParams, Method, Request, Response, Topic};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::client::open;
use crate::ControlClient;
use crate::testing::{Lab, TestNode};

async fn client(node: &TestNode) -> ControlClient {
    ControlClient::connect(&node.control_path).await.unwrap()
}

#[tokio::test]
async fn the_socket_speaks_newline_delimited_json_rpc() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let (read, mut write) = tokio::io::split(open(&node.control_path).await.unwrap());
    let mut lines = BufReader::new(read).lines();
    let request = serde_json::to_string(&Request::new(7, Method::Status, None)).unwrap();
    write
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    let line = lines.next_line().await.unwrap().unwrap();
    let response: Response = serde_json::from_str(&line).unwrap();
    assert_eq!((response.jsonrpc.as_str(), response.id), ("2.0", 7));
    assert!(response.error.is_none());
    assert_eq!(response.result.unwrap()["computer"]["name"], "Alpha");
}

#[cfg(unix)]
#[tokio::test]
async fn only_the_owning_user_can_connect() {
    use std::os::unix::fs::PermissionsExt;
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mode = std::fs::metadata(&node.control_path)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test]
async fn results_validate_against_the_mouser_proto_types() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut c = client(&node).await;
    let desk: Desk = c
        .call_typed(
            Method::DeskCreate,
            &DeskCreateParams {
                name: "Home".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(desk.name, "Home");
    let status: mouser_proto::control::Status =
        c.call_typed(Method::Status, &json!({})).await.unwrap();
    assert_eq!(status.desk.unwrap().id, desk.desk_id);
    let pending: Vec<mouser_proto::control::PendingJoin> =
        c.call_typed(Method::JoinPending, &json!({})).await.unwrap();
    assert!(pending.is_empty());
    let discovered: Vec<mouser_proto::control::DiscoveredDesk> = c
        .call_typed(Method::DesksDiscovered, &json!({}))
        .await
        .unwrap();
    assert!(discovered.is_empty());
    let _: mouser_proto::control::Empty =
        c.call_typed(Method::DeskLeave, &json!({})).await.unwrap();
}

#[tokio::test]
async fn errors_carry_json_rpc_codes() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut c = client(&node).await;
    // Unknown names and methods not built yet are "method not found".
    let (read, mut write) = tokio::io::split(open(&node.control_path).await.unwrap());
    let mut lines = BufReader::new(read).lines();
    write
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"nope\"}\n")
        .await
        .unwrap();
    let response: Response =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(response.error.unwrap().code, -32601);
    // A subscription belongs to a connection: called in process it is not
    // available.
    let err = crate::testing::call_err(&node, Method::Subscribe, json!({})).await;
    assert_eq!(err.code, -32601);
    // Bad params are "invalid params".
    let err = c
        .call(Method::DeskCreate, Some(json!({"name": 5})))
        .await
        .unwrap_err();
    assert_eq!(err.code, -32602);
    let err = c.call(Method::DeskCreate, None).await.unwrap_err();
    assert_eq!(err.code, -32602);
    let err = c
        .call(Method::Subscribe, Some(json!({"topics": ["bogus"]})))
        .await
        .unwrap_err();
    assert_eq!(err.code, -32602);
    // The connection survives errors.
    assert!(c.call(Method::Status, None).await.is_ok());
}

#[tokio::test]
async fn subscribers_receive_only_the_topics_they_asked_for() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut events = client(&node).await;
    events.subscribe(vec![Topic::Problem]).await.unwrap();
    let mut other = client(&node).await;
    // A status notification is not subscribed to; a problem is.
    let _: Desk = other
        .call_typed(Method::DeskCreate, &json!({"name": "Home"}))
        .await
        .unwrap();
    node.platform.set_permission(
        mouser_platform::PermissionKind::Accessibility,
        mouser_platform::PermissionState::Missing,
    );
    other.call(Method::Status, None).await.unwrap();
    let note = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        events.next_notification(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(note.method, Topic::Problem);
    assert_eq!(note.params["kind"], "missing_permission");
}

#[tokio::test]
async fn notifications_that_arrive_during_a_call_are_kept() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut c = client(&node).await;
    c.subscribe(vec![Topic::Status]).await.unwrap();
    let _: Desk = c
        .call_typed(Method::DeskCreate, &json!({"name": "Home"}))
        .await
        .unwrap();
    let _ = c.call(Method::Status, None).await.unwrap();
    let note = c.next_notification().await.unwrap();
    assert_eq!(note.method, Topic::Status);
    let status: Value = note.params;
    assert_eq!(status["desk"]["name"], "Home");
}

#[tokio::test]
async fn a_closed_server_ends_the_notification_stream() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut c = client(&node).await;
    node.engine.shutdown().await;
    // Shutting down ends every connection task, so the stream reports its end.
    let end = tokio::time::timeout(std::time::Duration::from_secs(2), c.next_notification()).await;
    assert!(end.unwrap().is_none());
}

#[test]
fn the_default_path_is_per_user() {
    // What `default_path` is while no service runs.
    let path = crate::control::server::default_path_with(false, &|key| std::env::var(key).ok());
    if cfg!(windows) {
        let name = path.to_string_lossy().into_owned();
        assert!(name.starts_with(r"\\.\pipe\mouser-"), "{name}");
        assert_ne!(name, crate::control::SERVICE_PIPE);
    } else {
        assert_eq!(path.file_name().unwrap(), "mouser.sock");
    }
}

#[test]
fn the_default_path_is_the_service_pipe_when_it_exists() {
    let path = crate::control::server::default_path_with(true, &|_| None);
    assert_eq!(path, std::path::PathBuf::from(r"\\.\pipe\mouser-service"));
    assert_eq!(crate::control::SERVICE_PIPE, r"\\.\pipe\mouser-service");
}

#[cfg(not(windows))]
#[test]
fn off_windows_there_is_no_service_pipe() {
    assert!(!crate::control::server::service_pipe_exists());
}

#[cfg(windows)]
#[test]
fn windows_control_paths_become_pipe_names() {
    use super::server::pipe_name;
    use std::path::Path;
    let pipe = Path::new(r"\\.\pipe\mouser-me");
    assert_eq!(pipe_name(pipe), pipe);
    let mapped = pipe_name(Path::new(r"C:\Temp\a b\mouser.sock"));
    assert_eq!(
        mapped.to_string_lossy(),
        r"\\.\pipe\mouser-C-Temp-a-b-mouser-sock"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_uses_application_support() {
    let path = super::server::default_path_from(|k| (k == "HOME").then(|| "/Users/me".to_string()));
    assert_eq!(
        path,
        std::path::PathBuf::from("/Users/me/Library/Application Support/Mouser/mouser.sock")
    );
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn linux_uses_the_runtime_dir() {
    let path = super::server::default_path_from(|k| {
        (k == "XDG_RUNTIME_DIR").then(|| "/run/user/1000".to_string())
    });
    assert_eq!(path, std::path::PathBuf::from("/run/user/1000/mouser.sock"));
}

#[tokio::test]
async fn a_client_that_disconnects_is_forgotten() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let watchers = node.engine.ctx.notify.receiver_count();
    let c = client(&node).await;
    crate::testing::eventually("the server to serve the client", || async {
        node.engine.ctx.notify.receiver_count() == watchers + 1
    })
    .await;
    drop(c);
    crate::testing::eventually("the server to let go", || async {
        node.engine.ctx.notify.receiver_count() == watchers
    })
    .await;
}

#[tokio::test]
async fn a_panic_in_one_request_is_an_internal_error_and_the_server_keeps_serving() {
    use mouser_platform::{Os, PlatformInfo, SessionType};
    use mouser_proto::control::Empty;
    use mouser_testkit::VirtualPlatform;

    use crate::EngineEvent;
    use crate::testing::{call, call_err, status};

    let platform = VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os: Os::Windows,
        version: "virtual".into(),
        session: SessionType::Native,
        limitations: Vec::new(),
    });
    platform.set_secure_attention_supported(true);
    let lab = Lab::new();
    let node = lab.start_with("Alpha", platform, |_| {}).await;
    let mut events = node.engine.events();

    // Over the socket, as the CLI sees it.
    let mut socket = client(&node).await;
    node.platform.panic_on_next_inject();
    let err = socket
        .call(Method::SendCad, Some(json!({})))
        .await
        .unwrap_err();
    assert_eq!(err.code, -32603);
    assert_eq!(
        events.recv().await.unwrap(),
        EngineEvent::Fault {
            r#where: "control".into()
        }
    );
    // The same connection and the in-process handle keep serving.
    let value = socket.call(Method::Status, None).await.unwrap();
    assert_eq!(value["computer"]["name"], "Alpha");
    assert_eq!(status(&node).await.computer.name, "Alpha");
    let _: Empty = call(&node, Method::SendCad, json!({})).await;

    // In process, as the app sees it.
    node.platform.panic_on_next_inject();
    assert_eq!(
        call_err(&node, Method::SendCad, json!({})).await.code,
        -32603
    );
    assert_eq!(status(&node).await.computer.name, "Alpha");
}

#[cfg(windows)]
mod service_pipe {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
        SE_KERNEL_OBJECT,
    };
    use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    use super::*;
    use crate::control::SERVICE_PIPE;
    use mouser_proto::wire::ComputerMode;

    /// The DACL of the pipe `name`, as SDDL.
    ///
    /// `GetNamedSecurityInfoW` rejects a pipe path (`\\.\pipe\...`) with
    /// `ERROR_BAD_PATHNAME`; a pipe's security is read from an open handle, so
    /// open one with `READ_CONTROL` and ask `GetSecurityInfo` for its DACL.
    fn dacl_of(name: &str) -> String {
        /// The standard right to read an object's security — enough for the DACL.
        const READ_CONTROL: u32 = 0x0002_0000;
        let wide: Vec<u16> = std::ffi::OsStr::new(name)
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: `wide` is NUL-terminated; the handle is closed below.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                READ_CONTROL,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        assert!(
            !handle.is_null() && handle != INVALID_HANDLE_VALUE,
            "the pipe opens for READ_CONTROL: {}",
            std::io::Error::last_os_error()
        );
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: `handle` is a live pipe handle with READ_CONTROL; the
        // descriptor it yields is freed below.
        let status = unsafe {
            GetSecurityInfo(
                handle,
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut descriptor,
            )
        };
        // SAFETY: the handle is no longer needed now the security is read.
        unsafe { CloseHandle(handle) };
        assert_eq!(status, 0, "the pipe's security is readable");
        let mut text = std::ptr::null_mut();
        // SAFETY: `descriptor` came from the call above.
        let converted = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(converted, 0);
        // SAFETY: `text` is a NUL-terminated string allocated by the call.
        let sddl = unsafe {
            let len = (0..).take_while(|i| *text.add(*i) != 0).count();
            String::from_utf16(std::slice::from_raw_parts(text, len)).unwrap()
        };
        // SAFETY: both were allocated by the calls above and are not used again.
        unsafe {
            LocalFree(text.cast());
            LocalFree(descriptor);
        }
        sddl
    }

    #[tokio::test]
    async fn an_engine_in_service_mode_serves_the_service_pipe_to_system_administrators_and_the_user()
     {
        let lab = Lab::new();
        let node = lab
            .start_as(
                "Alpha",
                mouser_platform::Os::Windows,
                ComputerMode::Service,
                "0.3.0",
            )
            .await;
        // It serves the service pipe, and that is now the default path.
        assert_eq!(
            crate::control::default_path(),
            std::path::PathBuf::from(SERVICE_PIPE)
        );
        let mut service = ControlClient::connect(std::path::Path::new(SERVICE_PIPE))
            .await
            .unwrap();
        let value = service.call(Method::Status, None).await.unwrap();
        assert_eq!(value["computer"]["name"], "Alpha");
        // Only SYSTEM, Administrators and the interactive user may open it,
        // each with full access. The pipe is created from `GA` (GENERIC_ALL),
        // but the object manager maps a file object's generic rights to their
        // specific form when it stores the descriptor, so the DACL reads back
        // as `FA` (FILE_ALL_ACCESS) — the same grant, spelled specifically.
        let dacl = dacl_of(SERVICE_PIPE);
        for ace in ["(A;;FA;;;SY)", "(A;;FA;;;BA)", "(A;;FA;;;IU)"] {
            assert!(dacl.contains(ace), "{dacl}");
        }
        assert_eq!(dacl.matches("(A;").count(), 3, "{dacl}");
        // Nobody else can take the name over: it is the first instance.
        let again = tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true)
            .create(SERVICE_PIPE);
        assert!(again.is_err());
        node.engine.shutdown().await;
    }
}

#[cfg(not(windows))]
#[test]
fn off_windows_the_default_path_is_always_the_users() {
    let env = |key: &str| std::env::var(key).ok();
    assert_eq!(
        crate::control::default_path(),
        crate::control::server::default_path_with(false, &env)
    );
}

/// The pipe's listener: it serves each client, and dropped (the engine shuts
/// down) it closes the pipe at once, so no client gets in after shutdown.
#[cfg(windows)]
mod the_pipe_listener {
    use super::super::server::{Listener, pipe_name};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::ClientOptions;

    /// A listener on its own pipe, already waiting for a client.
    async fn waiting(dir: &tempfile::TempDir) -> (Listener, std::path::PathBuf) {
        let name = pipe_name(dir.path());
        let mut listener = Listener::bind(dir.path(), false);
        tokio::select! {
            biased;
            _ = listener.accept() => panic!("no client connected"),
            () = std::future::ready(()) => {}
        }
        (listener, name)
    }

    /// `server` and `client` are the two ends of one connection.
    async fn connected(
        mut server: Box<dyn super::super::server::Duplex>,
        mut client: impl tokio::io::AsyncRead + Unpin,
    ) {
        server.write_all(b"x").await.unwrap();
        let mut seen = [0; 1];
        client.read_exact(&mut seen).await.unwrap();
        assert_eq!(&seen, b"x");
    }

    #[tokio::test]
    async fn a_client_that_comes_while_it_waits_is_served() {
        let dir = tempfile::tempdir().unwrap();
        let name = pipe_name(dir.path());
        let mut listener = Listener::bind(dir.path(), false);
        let (server, client) = tokio::join!(listener.accept(), async {
            // The listener's wait starts first.
            tokio::task::yield_now().await;
            ClientOptions::new().open(&name).unwrap()
        });
        connected(server, client).await;
    }

    #[tokio::test]
    async fn a_client_that_came_before_the_wait_is_served() {
        let dir = tempfile::tempdir().unwrap();
        let name = pipe_name(dir.path());
        let mut listener = Listener::bind(dir.path(), false);
        let client = ClientOptions::new().open(&name).unwrap();
        connected(listener.accept().await, client).await;
    }

    #[tokio::test]
    async fn once_dropped_while_it_waits_it_takes_no_client() {
        let dir = tempfile::tempdir().unwrap();
        let (listener, name) = waiting(&dir).await;
        drop(listener);
        assert!(!name.exists());
    }

    #[tokio::test]
    async fn dropped_as_a_client_gets_in_it_takes_no_other() {
        let dir = tempfile::tempdir().unwrap();
        let (listener, name) = waiting(&dir).await;
        let first = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&name)
            .expect("the waiting instance takes a client");
        drop(listener);
        assert!(!name.exists());
        drop(first);
    }
}
