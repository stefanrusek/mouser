//! Peer wire protocol v1 (contracts/wire-protocol.md): control messages,
//! motion datagram and length-prefixed postcard framing.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::control::{FixAction, Problem, ProblemKind};
use crate::ids::{ComputerId, DeskId, DevicePublicKey};
use crate::model::{DeskState, Display, DisplayId, Os, PermissionKind, PermissionState, Side};

/// Wire protocol version sent in `Hello`.
pub const PROTOCOL_VERSION: u16 = 1;

/// First Mouser version that understands the v2 messages (`LogsRequest`,
/// `DeskSettings`, `RestartElevated`) and the `PeerStatus.mode` field. A
/// computer sends them only to peers whose `Hello.mouser_version` is at least
/// this.
pub const V2_SINCE: &str = "0.3.0";

/// Peers with different protocol versions are `Incompatible`.
pub fn is_compatible(a: u16, b: u16) -> bool {
    a == b
}

/// Platform-neutral modifier state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// 128-bit random transfer id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransferId(pub [u8; 16]);

/// One file or directory in a transfer or file manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferItem {
    pub relative_path: String,
    pub size: u64,
    pub is_dir: bool,
}

/// Clipboard representation kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardKind {
    TextPlain,
    TextHtml,
    TextRtf,
    ImagePng,
    Files,
}

/// Inline (<= 1 MiB) or by reference (fetched with `ClipboardFetch`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipboardPayload {
    Inline(Vec<u8>),
    Reference { size: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardRepresentation {
    pub kind: ClipboardKind,
    pub payload: ClipboardPayload,
}

/// macOS Secure Input holder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecureInputHolder {
    pub pid: u32,
    pub app_name: String,
}

/// How a computer runs (data-model "mode").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputerMode {
    Installed,
    Portable,
    PortableElevated,
    Service,
    Headless,
}

/// Body of the peer `Status` message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerStatus {
    pub permissions: BTreeMap<PermissionKind, PermissionState>,
    pub limitations: Vec<String>,
    pub secure_input_holder: Option<SecureInputHolder>,
    pub problems: Vec<Problem>,
    /// Last field; absent from the 001 shape sent to peers below [`V2_SINCE`].
    pub mode: ComputerMode,
}

/// [`Problem`] as 0.2.x peers encode it: no `app`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemV1 {
    pub problem_id: String,
    pub kind: ProblemKind,
    pub computer: ComputerId,
    pub message: String,
    pub fix: Option<FixAction>,
    pub raised_at: String,
    pub cleared_at: Option<String>,
}

/// [`PeerStatus`] in the 001 shape, for peers below [`V2_SINCE`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerStatusV1 {
    pub permissions: BTreeMap<PermissionKind, PermissionState>,
    pub limitations: Vec<String>,
    pub secure_input_holder: Option<SecureInputHolder>,
    pub problems: Vec<ProblemV1>,
}

impl PeerStatus {
    /// The 001 shape of this status.
    pub fn to_v1(&self) -> PeerStatusV1 {
        PeerStatusV1 {
            permissions: self.permissions.clone(),
            limitations: self.limitations.clone(),
            secure_input_holder: self.secure_input_holder.clone(),
            problems: self
                .problems
                .iter()
                .map(|p| ProblemV1 {
                    problem_id: p.problem_id.clone(),
                    kind: p.kind,
                    computer: p.computer,
                    message: p.message.clone(),
                    fix: p.fix,
                    raised_at: p.raised_at.clone(),
                    cleared_at: p.cleared_at.clone(),
                })
                .collect(),
        }
    }
}

impl PeerStatusV1 {
    /// The current shape, once the sender's mode is known.
    pub fn with_mode(self, mode: ComputerMode) -> PeerStatus {
        PeerStatus {
            permissions: self.permissions,
            limitations: self.limitations,
            secure_input_holder: self.secure_input_holder,
            problems: self
                .problems
                .into_iter()
                .map(|p| Problem {
                    problem_id: p.problem_id,
                    kind: p.kind,
                    computer: p.computer,
                    message: p.message,
                    app: None,
                    fix: p.fix,
                    raised_at: p.raised_at,
                    cleared_at: p.cleared_at,
                })
                .collect(),
            mode,
        }
    }
}

/// `ControlMessage::Status` variant index (0.2.8, unchanged).
const STATUS_VARIANT: u32 = 28;

/// Number of `ControlMessage` variants this build knows; a frame naming a
/// higher index is `FrameError::UnknownVariant`.
const CONTROL_VARIANTS: u32 = 34;

/// The `Status` frame exactly as a 0.2.x peer decodes it (001 shape), for
/// peers below [`V2_SINCE`].
pub fn encode_status_v1(status: &PeerStatus) -> Vec<u8> {
    let mut body = postcard::to_stdvec(&STATUS_VARIANT).expect("index serializes");
    body.extend(postcard::to_stdvec(&status.to_v1()).expect("status serializes"));
    let len = u32::try_from(body.len()).expect("frame fits in u32");
    let mut out = len.to_be_bytes().to_vec();
    out.extend(body);
    out
}

/// Decode a `Status` frame as a 0.2.x peer encodes it; returns the 001-shaped
/// status and bytes consumed. The frame carries no mode: the caller supplies
/// it with [`PeerStatusV1::with_mode`]. Any other message is `Malformed`.
pub fn decode_status_v1(buf: &[u8]) -> Result<(PeerStatusV1, usize), FrameError> {
    let (prefix, rest) = buf.split_first_chunk::<4>().ok_or(FrameError::Incomplete)?;
    let len = u32::from_be_bytes(*prefix) as usize;
    let body = rest.get(..len).ok_or(FrameError::Incomplete)?;
    let (variant, status) =
        postcard::take_from_bytes::<u32>(body).map_err(|_| FrameError::Malformed)?;
    if variant != STATUS_VARIANT {
        return Err(FrameError::Malformed);
    }
    let status = postcard::from_bytes(status).map_err(|_| FrameError::Malformed)?;
    Ok((status, 4 + len))
}

/// Card identifying a computer in a [`LogsPart`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsCard {
    pub id: ComputerId,
    pub name: String,
    pub os: Os,
    pub os_version: String,
    pub version: String,
    pub mode: ComputerMode,
}

/// Header of the logs uni stream (wire-protocol-delta "Logs stream"); the
/// file bytes follow it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogsPart {
    pub card: LogsCard,
    /// Verbatim `config.toml`; holds no secrets.
    pub config_toml: String,
    /// `false` means `files` is empty (reason `logging_off`).
    pub logging: bool,
    /// `(mouser.log | mouser.log.1, length)`.
    pub files: Vec<(String, u64)>,
}

/// Logs stream id: first 16 bytes of SHA-256 over `"logs\n"` and the request.
pub fn logs_stream_id(request: &[u8; 16]) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(b"logs\n");
    hasher.update(request);
    let digest = hasher.finalize();
    digest[..16].try_into().expect("digest is 32 bytes")
}

/// Every message on the reliable control stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ControlMessage {
    Hello {
        protocol_version: u16,
        mouser_version: String,
        computer_id: ComputerId,
        display_name: String,
        os: Os,
        desk_id: Option<DeskId>,
    },
    JoinRequest {
        desk_id: DeskId,
        device_public_key: DevicePublicKey,
        display_name: String,
        os: Os,
    },
    JoinDecision {
        approved: bool,
        /// `sealed(desk_key, desk snapshot)`; present iff approved.
        sealed: Option<Vec<u8>>,
    },
    /// HMAC(desk_key, handshake transcript).
    DeskAuth {
        hmac: [u8; 32],
    },
    DeskState(DeskState),
    Leave {
        computer_id: ComputerId,
    },
    Remove {
        computer_id: ComputerId,
    },
    Displays {
        displays: Vec<Display>,
    },
    Enter {
        display_id: DisplayId,
        x: f32,
        y: f32,
        seq_base: u64,
        modifiers_down: Modifiers,
    },
    LeaveControl {
        edge: Side,
        position: f32,
    },
    Key {
        seq: u64,
        /// `(usage_page << 16) | usage_id`, e.g. keyboard A = `0x0007_0004`.
        usage: u32,
        down: bool,
        modifiers: Modifiers,
    },
    Button {
        seq: u64,
        /// 1-5.
        button: u8,
        down: bool,
    },
    Scroll {
        seq: u64,
        dx: i32,
        dy: i32,
    },
    ReleaseAll {
        seq: u64,
    },
    Panic,
    ClipboardOffer {
        sequence: u64,
        representations: Vec<ClipboardRepresentation>,
        file_manifest: Vec<TransferItem>,
    },
    ClipboardFetch {
        sequence: u64,
        kind: ClipboardKind,
    },
    ClipboardData {
        sequence: u64,
        kind: ClipboardKind,
        bytes: Vec<u8>,
    },
    TransferOffer {
        transfer_id: TransferId,
        items: Vec<TransferItem>,
        destination_dir: String,
    },
    TransferAccept {
        transfer_id: TransferId,
    },
    TransferReject {
        transfer_id: TransferId,
        reason: String,
    },
    TransferConflict {
        transfer_id: TransferId,
        item: String,
    },
    TransferResolve {
        transfer_id: TransferId,
        overwrite: bool,
    },
    TransferDone {
        transfer_id: TransferId,
    },
    QuickJumpShow {
        letters: Vec<(DisplayId, char)>,
    },
    QuickJumpHide,
    /// The signed `latest.json` text, byte-for-byte (see `release::ReleaseManifest`).
    ReleaseAvailable {
        manifest_json: String,
    },
    ReleaseFetch {
        version: String,
        asset: String,
    },
    Status(PeerStatus),
    SecureAttention,
    /// This computer is paused (or resumed): isolated from the desk as if
    /// disconnected, links kept. Appended last; sent only to peers running
    /// 0.2.2 or later, whose older versions cannot decode it.
    Paused {
        paused: bool,
    },
    /// Appended after `Paused`; v2 (peers at least [`V2_SINCE`]). Ask a peer
    /// for its part of a log bundle on a uni stream.
    LogsRequest {
        request: [u8; 16],
    },
    /// Desk-shared settings with their epoch; v2.
    DeskSettings {
        epoch: u64,
        author: ComputerId,
        landing_highlight: bool,
        command_as_control: bool,
    },
    /// Ask a portable, non-elevated Windows computer to restart elevated; v2.
    RestartElevated,
}

/// `display_id_hash` for [`MotionDatagram`]: first 4 bytes (big-endian) of
/// SHA-256 over the 16 computer-id bytes followed by the UTF-8 display key.
pub fn display_id_hash(display_id: &DisplayId) -> u32 {
    let mut hasher = Sha256::new();
    hasher.update(display_id.computer.0);
    hasher.update(display_id.key.as_bytes());
    let digest = hasher.finalize();
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
}

/// Unreliable pointer-motion datagram, latest-wins.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotionDatagram {
    pub seq: u64,
    pub display_id_hash: u32,
    pub x: f32,
    pub y: f32,
}

/// Framing failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// The buffer does not yet hold a whole frame.
    Incomplete,
    /// The frame body is not a valid message.
    Malformed,
    /// A valid frame that names a `ControlMessage` variant this build does
    /// not know; the reader skips it (wire-protocol-delta "Reader rule").
    UnknownVariant(u32),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incomplete => f.write_str("incomplete frame"),
            Self::Malformed => f.write_str("malformed frame"),
            Self::UnknownVariant(index) => write!(f, "unknown message variant {index}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Encode as a 4-byte big-endian length followed by the postcard body.
pub fn encode_frame<T: Serialize>(msg: &T) -> Vec<u8> {
    let body = postcard::to_stdvec(msg).expect("message serializes");
    let len = u32::try_from(body.len()).expect("frame fits in u32");
    let mut out = len.to_be_bytes().to_vec();
    out.extend(body);
    out
}

/// Decode one frame from the front of `buf`; returns the message and bytes consumed.
pub fn decode_frame<T: DeserializeOwned>(buf: &[u8]) -> Result<(T, usize), FrameError> {
    let (prefix, rest) = buf.split_first_chunk::<4>().ok_or(FrameError::Incomplete)?;
    let len = u32::from_be_bytes(*prefix) as usize;
    let body = rest.get(..len).ok_or(FrameError::Incomplete)?;
    let msg = postcard::from_bytes(body).map_err(|error| match error {
        // An out-of-range enum index is a custom serde error; it is an
        // unknown `ControlMessage` variant when the leading index is past the
        // last one this build knows, and malformed otherwise.
        postcard::Error::SerdeDeCustom => {
            let (index, _) = postcard::take_from_bytes::<u32>(body)
                .expect("a custom error is raised after the variant index was read");
            if index >= CONTROL_VARIANTS {
                FrameError::UnknownVariant(index)
            } else {
                FrameError::Malformed
            }
        }
        _ => FrameError::Malformed,
    })?;
    Ok((msg, 4 + len))
}
