//! The local control server: newline-delimited JSON-RPC 2.0 over a Unix
//! domain socket (0600, so only the same OS user connects) or, on Windows, a
//! named pipe that rejects remote clients.

use crate::lock::LockExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mouser_proto::control::{
    ConfigGetParams, ConfigSetParams, DeskCreateParams, DeskJoinParams, DeskMoveParams, Empty,
    JoinDecideParams, LoggingSetParams, LogsCollectParams, Method, RemoveMemberParams, Request,
    Response, RpcError, SetOrigin, SettingOrigin, SubscribeParams, Topic,
};
#[cfg(windows)]
use mouser_proto::wire::ComputerMode;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::control::{invalid, status};
use crate::discovery::group_by_desk;
use crate::engine::Ctx;
use crate::faults::{self, Faulted, RecoveryAction, run_unit_async};

/// A connected control client.
pub(crate) trait Duplex: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Duplex for T {}

fn parse<T: DeserializeOwned>(params: Option<Value>) -> Result<T, RpcError> {
    serde_json::from_value(params.unwrap_or(Value::Null)).map_err(|e| invalid(e.to_string()))
}

fn json<T: Serialize>(value: T) -> Result<Value, RpcError> {
    Ok(serde_json::to_value(value).expect("result serializes"))
}

/// Run one method. A panic in it is that request's internal error (JSON-RPC
/// `-32603`); the server keeps serving later requests.
pub(crate) async fn call(
    ctx: &Arc<Ctx>,
    name: &str,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    // Nobody is on the other end of an in-process call to take chunks.
    let (chunks, _) = mpsc::unbounded_channel();
    call_streaming(ctx, name, params, &chunks).await
}

/// [`call`] for a caller that is a connection: whatever the method sends
/// to it before it answers goes down `chunks`.
async fn call_streaming(
    ctx: &Arc<Ctx>,
    name: &str,
    params: Option<Value>,
    chunks: &UnboundedSender<Value>,
) -> Result<Value, RpcError> {
    run_unit_async(&ctx.events, "control", dispatch(ctx, name, params, chunks))
        .await
        .unwrap_or_else(|Faulted| {
            faults::recovered(&ctx.log, "control", RecoveryAction::Continued);
            Err(RpcError {
                code: -32603,
                message: "Internal error: the request failed inside Mouser.".into(),
                data: None,
            })
        })
}

async fn dispatch(
    ctx: &Arc<Ctx>,
    name: &str,
    params: Option<Value>,
    chunks: &UnboundedSender<Value>,
) -> Result<Value, RpcError> {
    let unknown = |message: String| RpcError {
        code: -32601,
        message,
        data: None,
    };
    let method =
        Method::from_name(name).ok_or_else(|| unknown(format!("unknown method {name}")))?;
    match method {
        Method::Status => json(status::build(ctx)),
        Method::DesksDiscovered => {
            let mine = ctx.core.guard().state.desk.as_ref().map(|d| d.desk_id);
            json(group_by_desk(&ctx.discovery.found(), mine))
        }
        Method::DeskCreate => {
            let p: DeskCreateParams = parse(params)?;
            json(ctx.create_desk(&p.name)?)
        }
        Method::DeskJoin => {
            let p: DeskJoinParams = parse(params)?;
            json(ctx.join(p, false).await?)
        }
        Method::DeskMove => {
            let p: DeskMoveParams = parse(params)?;
            json(
                ctx.join(DeskJoinParams::ById { desk_id: p.desk_id }, true)
                    .await?,
            )
        }
        Method::DeskLeave => {
            ctx.leave_desk()?;
            json(Empty {})
        }
        Method::DeskRemoveMember => {
            let p: RemoveMemberParams = parse(params)?;
            ctx.remove_member(p.computer_id)?;
            json(Empty {})
        }
        Method::DeskRename => {
            ctx.rename(parse(params)?)?;
            json(Empty {})
        }
        Method::JoinPending => json(ctx.pending_joins()),
        Method::JoinDecide => {
            let p: JoinDecideParams = parse(params)?;
            ctx.decide(p.request_id, p.approve)?;
            json(Empty {})
        }
        Method::ActivePause | Method::ActiveResume => {
            ctx.set_paused(method == Method::ActivePause);
            json(Empty {})
        }
        Method::SendCad => {
            ctx.send_cad()?;
            json(Empty {})
        }
        Method::LayoutGet => Ok(ctx.get_layout()),
        Method::LayoutSet => ctx.set_layout(parse(params)?),
        Method::LayoutKeepPlacement => ctx.keep_placement(parse(params)?),
        Method::ConfigGet => {
            let p: ConfigGetParams = parse(Some(params.unwrap_or_else(|| json!({}))))?;
            ctx.get_config(p.key.as_deref())
        }
        Method::ConfigSet => {
            let p: ConfigSetParams = parse(params)?;
            let origin = match p.origin {
                SetOrigin::App => SettingOrigin::App,
                SetOrigin::Cli => SettingOrigin::Cli,
            };
            ctx.set_config(&p.key, p.value, origin)?;
            json(Empty {})
        }
        Method::LoggingSet => {
            let p: LoggingSetParams = parse(params)?;
            json(ctx.set_logging(p.enabled)?)
        }
        Method::LogsCollect => {
            let p: LogsCollectParams = parse(Some(params.unwrap_or_else(|| json!({}))))?;
            json(ctx.logs_collect(p).await?)
        }
        Method::LogsCollectStream => json(ctx.logs_collect_stream(chunks).await?),
        Method::ElevationRestart => ctx.elevation_restart(parse(params)?),
        Method::UpdateCheck => json(ctx.update_check().await?),
        Method::UpdateApply => json(ctx.update_apply().await?),
        _ => Err(unknown(format!("{name} is not available yet"))),
    }
}

async fn write_line(write: &mut (impl AsyncWrite + Unpin), value: &impl Serialize) {
    let mut line = serde_json::to_string(value).expect("message serializes");
    line.push('\n');
    // A vanished client is noticed by the next read.
    let _ = write.write_all(line.as_bytes()).await;
}

async fn serve_connection(ctx: Arc<Ctx>, stream: Box<dyn Duplex>) {
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    let mut notifications = ctx.notify.subscribe();
    let mut topics: Vec<Topic> = Vec::new();
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line.expect("control connection readable") else {
                    break;
                };
                let request: Request = serde_json::from_str(&line).expect("well-formed JSON-RPC request");
                let (chunks, mut sent) = mpsc::unbounded_channel();
                let result = if request.method == Method::Subscribe.name() {
                    parse::<SubscribeParams>(request.params).map(|p| {
                        topics = p.topics;
                        json!({})
                    })
                } else {
                    call_streaming(&ctx, &request.method, request.params, &chunks).await
                };
                // What was sent to this connection comes before the answer.
                while let Ok(chunk) = sent.try_recv() {
                    write_line(&mut write, &chunk).await;
                }
                let response = match result {
                    Ok(value) => Response::ok(request.id, value),
                    Err(error) => Response::err(request.id, error),
                };
                write_line(&mut write, &response).await;
            }
            note = notifications.recv() => {
                let note = note.expect("notifications are consumed in time");
                if topics.contains(&note.method) {
                    write_line(&mut write, &note).await;
                }
            }
        }
    }
}

#[cfg(unix)]
struct Listener(tokio::net::UnixListener);

#[cfg(unix)]
impl Listener {
    fn bind(path: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        // A socket left by a crashed run would make bind fail.
        let _ = std::fs::remove_file(path);
        let listener = tokio::net::UnixListener::bind(path).expect("control socket binds");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("control socket permissions");
        Self(listener)
    }

    async fn accept(&mut self) -> Box<dyn Duplex> {
        Box::new(self.0.accept().await.expect("control socket accepts").0)
    }
}

/// The pipe a control path stands for: a path already in the pipe namespace is
/// used as is, any other path (a test's temp directory) becomes a pipe name
/// made of its alphanumerics, so every distinct path has its own pipe.
#[cfg(windows)]
pub(crate) fn pipe_name(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if text.starts_with(r"\\.\pipe\") {
        return path.to_path_buf();
    }
    let flat: String = text
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let flat = flat
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    PathBuf::from(format!(r"\\.\pipe\mouser-{flat}"))
}

/// Who may open the service's pipe: SYSTEM, Administrators and the
/// interactive user, each with full access; nobody else.
#[cfg(windows)]
const SERVICE_PIPE_SDDL: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;IU)";

#[cfg(windows)]
pub(crate) struct Listener {
    name: PathBuf,
    /// The pipe is the service's: it gets [`SERVICE_PIPE_SDDL`].
    service: bool,
    next: tokio::net::windows::named_pipe::NamedPipeServer,
}

#[cfg(windows)]
impl Listener {
    fn instance(
        name: &Path,
        first: bool,
        service: bool,
    ) -> tokio::net::windows::named_pipe::NamedPipeServer {
        let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
        options
            .first_pipe_instance(first)
            .reject_remote_clients(true);
        let pipe = if service {
            Self::create_for_service(&mut options, name)
        } else {
            options.create(name)
        };
        pipe.expect("control pipe creates")
    }

    fn create_for_service(
        options: &mut tokio::net::windows::named_pipe::ServerOptions,
        name: &Path,
    ) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
        let sddl: Vec<u16> = SERVICE_PIPE_SDDL.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: `sddl` is NUL-terminated; the descriptor it yields is freed
        // below, after the pipe has copied it.
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(
            converted, 0,
            "the service pipe's security descriptor parses"
        );
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        // SAFETY: `attributes` points at a valid descriptor for the call.
        let pipe = unsafe {
            options.create_with_security_attributes_raw(name, (&raw mut attributes).cast())
        };
        // SAFETY: allocated by the conversion above and not used again.
        unsafe { LocalFree(descriptor) };
        pipe
    }

    pub(crate) fn bind(path: &Path, service: bool) -> Self {
        let name = pipe_name(path);
        Self {
            next: Self::instance(&name, true, service),
            name,
            service,
        }
    }

    pub(crate) async fn accept(&mut self) -> Box<dyn Duplex> {
        self.next.connect().await.expect("control pipe accepts");
        let connected = std::mem::replace(
            &mut self.next,
            Self::instance(&self.name, false, self.service),
        );
        Box::new(connected)
    }
}

/// Bind the control endpoint (before returning, so clients can connect at
/// once) and serve clients until the engine shuts down.
pub(crate) fn serve(ctx: &Arc<Ctx>, path: &Path) {
    // The service's engine serves its pipe, for every user's Mouser.
    #[cfg(windows)]
    let mut listener = match ctx.mode {
        ComputerMode::Service => Listener::bind(Path::new(super::SERVICE_PIPE), true),
        _ => Listener::bind(path, false),
    };
    #[cfg(unix)]
    let mut listener = Listener::bind(path);
    let owner = ctx.clone();
    ctx.spawn(async move {
        loop {
            let stream = listener.accept().await;
            owner.spawn(serve_connection(owner.clone(), stream));
        }
    });
}

/// Remove the socket file left behind (pipes vanish with their server).
#[cfg(unix)]
pub(crate) fn cleanup(path: &Path) {
    std::fs::remove_file(path).expect("control socket file exists until shutdown");
}

#[cfg(windows)]
pub(crate) fn cleanup(_path: &Path) {}

/// Whether the service's pipe exists.
#[cfg(windows)]
pub(crate) fn service_pipe_exists() -> bool {
    Path::new(super::SERVICE_PIPE).exists()
}

/// Only Windows has a service pipe.
#[cfg(unix)]
pub(crate) fn service_pipe_exists() -> bool {
    false
}

/// The service's pipe when `service_pipe_exists`, otherwise the endpoint of
/// this user.
pub(crate) fn default_path_with(
    service_pipe_exists: bool,
    env: &dyn Fn(&str) -> Option<String>,
) -> PathBuf {
    if service_pipe_exists {
        PathBuf::from(super::SERVICE_PIPE)
    } else {
        default_path_from(env)
    }
}

/// The per-OS control endpoint of this user (contracts/control-api.md).
#[cfg(target_os = "macos")]
pub(crate) fn default_path_from(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    PathBuf::from(env("HOME").expect("HOME is set"))
        .join("Library/Application Support/Mouser/mouser.sock")
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn default_path_from(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    PathBuf::from(
        env("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR must be set (plain sudo strips it)"),
    )
    .join("mouser.sock")
}

/// The pipe is named after the user; the contract names the user's SID, which
/// needs the Windows API (see the lane report).
#[cfg(windows)]
pub(crate) fn default_path_from(env: impl Fn(&str) -> Option<String>) -> PathBuf {
    PathBuf::from(format!(
        r"\\.\pipe\mouser-{}",
        env("USERNAME").expect("USERNAME is set")
    ))
}
