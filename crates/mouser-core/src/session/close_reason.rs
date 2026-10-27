//! Why a link ended (data-model "Link loss reason") and what this computer
//! says when it closes one. The reason comes from the connection's error
//! and the close reason bytes; it feeds `peer.disconnected`.

use quinn::ConnectionError;

use crate::logging::events::tagged;

tagged! {
    /// Why a desk link was lost.
    LinkLossReason {
        PeerLeft => "peer_left",
        PeerQuit => "peer_quit",
        Removed => "removed",
        Replaced => "replaced",
        Unreachable => "unreachable",
        Incompatible => "incompatible",
        Protocol => "protocol",
        FaultHere => "fault_here",
        QuitHere => "quit_here",
    }
}

impl LinkLossReason {
    /// The reason the connection ended as `error` says. A close this
    /// computer made itself carries its own reason (see [`CloseReason`]).
    pub(crate) fn of(error: &ConnectionError) -> Self {
        match error {
            ConnectionError::ApplicationClosed(close) => match &close.reason[..] {
                // 0.2.x computers say `shutdown` when they quit.
                b"quit" | b"shutdown" => Self::PeerQuit,
                b"not a member of this desk" => Self::Removed,
                b"replaced" => Self::Replaced,
                b"incompatible" => Self::Incompatible,
                b"protocol" => Self::Protocol,
                // `done`, a peer whose link task faulted, and anything a
                // newer version says: the peer closed the link.
                _ => Self::PeerLeft,
            },
            ConnectionError::VersionMismatch => Self::Incompatible,
            ConnectionError::LocallyClosed => Self::PeerLeft,
            // Idle timeout, path lost, reset.
            _ => Self::Unreachable,
        }
    }
}

/// What this computer says when it closes a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseReason {
    /// The link is finished (a computer left the desk).
    Done,
    /// This computer is quitting.
    Quit,
    /// A unit of work on this link faulted.
    Fault,
    /// The peer sent something that is not part of the protocol.
    Protocol,
    /// A newer connection to the same computer took over.
    Replaced,
    /// The peer failed the desk check.
    NotMember,
}

impl CloseReason {
    /// The close reason bytes on the wire.
    pub(crate) fn bytes(self) -> &'static [u8] {
        match self {
            Self::Done => b"done",
            Self::Quit => b"quit",
            Self::Fault => b"fault",
            Self::Protocol => b"protocol",
            Self::Replaced => b"replaced",
            Self::NotMember => b"not a member of this desk",
        }
    }

    /// How this computer logs a link it closed this way.
    pub(crate) fn loss(self) -> LinkLossReason {
        match self {
            Self::Done => LinkLossReason::PeerLeft,
            Self::Quit => LinkLossReason::QuitHere,
            Self::Fault => LinkLossReason::FaultHere,
            Self::Protocol => LinkLossReason::Protocol,
            Self::Replaced => LinkLossReason::Replaced,
            Self::NotMember => LinkLossReason::Removed,
        }
    }
}
