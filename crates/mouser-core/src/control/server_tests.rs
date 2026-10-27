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
    let path = crate::control::default_path();
    if cfg!(windows) {
        let name = path.to_string_lossy().into_owned();
        assert!(name.starts_with(r"\\.\pipe\mouser-"), "{name}");
    } else {
        assert_eq!(path.file_name().unwrap(), "mouser.sock");
    }
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
