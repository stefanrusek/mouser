//! Peer wire protocol v1 (contracts/wire-protocol.md): control messages,
//! motion datagram and length-prefixed postcard framing.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::control::Problem;
use crate::ids::{ComputerId, DeskId, DevicePublicKey};
use crate::model::{DeskState, Display, DisplayId, Os, PermissionKind, PermissionState, Side};

/// Wire protocol version sent in `Hello`.
pub const PROTOCOL_VERSION: u16 = 1;

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

/// Body of the peer `Status` message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerStatus {
    pub permissions: BTreeMap<PermissionKind, PermissionState>,
    pub limitations: Vec<String>,
    pub secure_input_holder: Option<SecureInputHolder>,
    pub problems: Vec<Problem>,
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
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Incomplete => "incomplete frame",
            Self::Malformed => "malformed frame",
        })
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
    let msg = postcard::from_bytes(body).map_err(|_| FrameError::Malformed)?;
    Ok((msg, 4 + len))
}
