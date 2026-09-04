//! What the app and the agent say to each other.
//!
//! One framing rule: a 4-byte little-endian length, then a CBOR body. Nothing
//! is line-delimited and nothing is text, because the same stream carries file
//! contents and PNG bytes, and a framing that has to escape its payload is a
//! framing that will one day fail to.
//!
//! CBOR rather than protobuf: there is exactly one client and one agent, both
//! built from this repository at the same version, and the version match is
//! established before a byte is sent. That makes a schema language a cost with
//! no matching benefit — `serde` derives on the types the app already has are
//! free, and byte strings survive without base64.

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::contract::{
    ClaudeChannelStatus, CodexChannelStatus, Commit, FullFileContents, RepoDiff,
};
use crate::services::icon_scan::Candidate;
use crate::services::repository::FileMeta;

/// The protocol this build speaks. Not a negotiation: the agent binary is
/// version-matched by filename before it is ever run, so a mismatch here means
/// something is very wrong and the connection should fail loudly rather than
/// try to find common ground.
pub const PROTOCOL_VERSION: u32 = 1;

/// The largest frame either side will send or accept.
///
/// A working-tree file is the biggest thing that legitimately crosses, and the
/// diff view caps those at 64 MiB. The margin above that is for the envelope,
/// not for a bigger payload — and the cap is enforced on read so a corrupted
/// length prefix cannot make the reader allocate a gigabyte.
pub const MAX_FRAME_BYTES: u32 = 80 * 1024 * 1024;

/// Correlates a response with its request. Zero is reserved for the agent's
/// unsolicited frames, which answer nothing.
pub type RequestId = u32;

/// The id every unsolicited frame carries.
pub const EVENT_ID: RequestId = 0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope<T> {
    pub id: RequestId,
    pub body: T,
}

/// Everything the app can ask the machine a repository is on.
///
/// Deliberately coarse. `Diff` returns the whole collected diff rather than
/// exposing `git` and letting the app run the walk one patch at a time: the
/// walk costs one invocation per changed file, and doing that over a network is
/// the difference between one round trip and several hundred.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Request {
    /// A liveness check that also proves the version match.
    Hello { protocol: u32 },
    /// Collected metadata for every change in the repository.
    Diff { root: String },
    /// Full before/after text for one file.
    FileContents {
        root: String,
        path: String,
        old_path: Option<String>,
        status: crate::contract::ChangeStatus,
        staged: bool,
    },
    History { root: String, limit: Option<f64> },
    ListFiles { root: String },
    StageFile {
        root: String,
        path: String,
        old_path: Option<String>,
    },
    CommitAll { root: String, message: String },
    /// The complete annotated diff the commit-message model is shown. Built on
    /// the host so the patches never cross twice.
    CommitMessageDiff { root: String },
    IconCandidates { root: String },
    /// Whether a Claude Code session is listening on the host.
    ClaudeStatus { root: String },
    /// Hand a message to that session. One direction only.
    ClaudeSend { root: String, message: String },
    /// Whether a Codex session has worked in that repository.
    CodexStatus { root: String },
    /// Queue a message for it. One direction only, and it does not require the
    /// session to be running: Codex holds the message until that thread does.
    CodexSend { root: String, message: String },
    /// Put a pasted image where that session can open it. The bytes cross
    /// once, here; the message that follows carries only the path they landed
    /// at, because a path is the only form of an image that means anything on
    /// the far side of a connection.
    WriteAttachment {
        root: String,
        #[serde(with = "serde_bytes_vec")]
        bytes: Vec<u8>,
    },
    /// One `git` invocation, for the handful of things the app asks that have
    /// no richer request. Kept narrow on purpose.
    Git { root: String, args: Vec<String> },
    ReadFile {
        root: String,
        path: String,
        max_bytes: u64,
    },
    Metadata { root: String, path: String },
    /// Is this a git repository at all, and where is its root?
    ResolveRepository { path: String },
    /// Start watching. Changes arrive as `Event::RepoChanged` until `Unwatch`.
    Watch { root: String },
    Unwatch { root: String },
    /// Close the connection. The agent exits when its stdin closes too; this is
    /// the orderly version, so the app can tell a clean shutdown from a drop.
    Shutdown,
}

/// One answer. `Err` carries the message the app will show, already phrased for
/// a person — the agent knows what it was doing and the app does not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Response {
    Hello {
        protocol: u32,
        agent_version: String,
    },
    Diff(RepoDiff),
    FileContents(FullFileContents),
    History(Vec<Commit>),
    ListFiles(Vec<String>),
    Unit,
    Commit(String),
    CommitMessageDiff(String),
    IconCandidates(Vec<Candidate>),
    ClaudeStatus(ClaudeChannelStatus),
    /// The channel's message id, useful only for correlating logs.
    ClaudeSent(String),
    CodexStatus(CodexChannelStatus),
    /// The id Codex gave the queued message, useful only for correlating logs.
    CodexSent(String),
    /// Where a pasted image was written, in the host's own path style.
    Attachment(String),
    Git(String),
    Bytes(#[serde(with = "serde_bytes_vec")] Vec<u8>),
    Metadata(Option<FileMeta>),
    /// The repository root above the requested path, or `None` when there is
    /// no `.git` at or above it.
    Repository(Option<String>),
    Err {
        /// The `AppError` tag, so the app can rebuild the same variant rather
        /// than flattening every remote failure into one kind.
        tag: String,
        message: String,
    },
}

/// A frame the agent sends without being asked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Event {
    /// A watched repository changed, already debounced on the host so a
    /// thirty-file rewrite is one frame rather than thirty.
    RepoChanged { root: String },
}

/// What travels in a frame: an answer to something, or news.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Message {
    Request(Envelope<Request>),
    Response(Envelope<Response>),
    Event(Event),
}

/// `Vec<u8>` through CBOR's byte-string type rather than as an array of
/// integers, which is the difference between one byte per byte and up to three.
mod serde_bytes_vec {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        // `serde_bytes`' own newtype would be another dependency for one field;
        // CBOR decodes a byte string into `Vec<u8>` directly.
        Vec::<u8>::deserialize(deserializer)
    }
}

#[derive(Debug)]
pub enum FrameError {
    /// The stream ended cleanly between frames. Not a failure: it is how both
    /// sides say goodbye.
    Closed,
    /// A length prefix larger than anything this protocol sends. Refused before
    /// the buffer is grown, so a corrupt or hostile stream cannot exhaust
    /// memory by claiming a frame is four gigabytes.
    TooLarge(u32),
    Io(std::io::Error),
    Encoding(String),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "the connection closed"),
            Self::TooLarge(len) => write!(
                f,
                "a frame claimed {len} bytes; the limit is {MAX_FRAME_BYTES}"
            ),
            Self::Io(error) => write!(f, "{error}"),
            Self::Encoding(detail) => write!(f, "malformed frame: {detail}"),
        }
    }
}

impl std::error::Error for FrameError {}

impl From<std::io::Error> for FrameError {
    fn from(error: std::io::Error) -> Self {
        // An `ssh` that dies takes the pipe with it, and both of these are how
        // that arrives. Reporting them as "closed" is what lets the caller
        // reconnect rather than show an I/O error nobody can act on.
        match error.kind() {
            std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe => Self::Closed,
            _ => Self::Io(error),
        }
    }
}

pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &Message,
) -> Result<(), FrameError> {
    let mut body = Vec::new();
    ciborium::into_writer(message, &mut body).map_err(|error| FrameError::Encoding(error.to_string()))?;
    let len = u32::try_from(body.len()).map_err(|_| FrameError::TooLarge(u32::MAX))?;
    if len > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(len));
    }
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(&body).await?;
    // Without this the frame can sit in the pipe buffer while both sides wait
    // on each other, which is a deadlock that only appears under small writes.
    writer.flush().await?;
    Ok(())
}

pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Message, FrameError> {
    let mut header = [0u8; 4];
    reader.read_exact(&mut header).await?;
    let len = u32::from_le_bytes(header);
    if len > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = vec![0u8; len as usize];
    reader.read_exact(&mut body).await?;
    ciborium::from_reader(body.as_slice()).map_err(|error| FrameError::Encoding(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(message: Message) -> Message {
        let mut buffer = Vec::new();
        let write = write_frame(&mut buffer, &message);
        futures::executor::block_on(write).expect("write");
        futures::executor::block_on(read_frame(&mut buffer.as_slice())).expect("read")
    }

    #[test]
    fn a_request_survives_the_wire_unchanged() {
        let message = Message::Request(Envelope {
            id: 42,
            body: Request::Diff {
                root: "/srv/app".into(),
            },
        });

        assert_eq!(roundtrip(message.clone()), message);
    }

    #[test]
    fn bytes_cross_as_bytes_rather_than_as_a_list_of_numbers() {
        let payload: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        let message = Message::Response(Envelope {
            id: 7,
            body: Response::Bytes(payload.clone()),
        });

        let mut buffer = Vec::new();
        futures::executor::block_on(write_frame(&mut buffer, &message)).expect("write");

        // A CBOR byte string is its length plus a short header. An array of
        // integers would be at least twice this, and that difference is every
        // file the app ever reads off a remote host.
        assert!(
            buffer.len() < payload.len() + 64,
            "4096 bytes encoded to {} bytes",
            buffer.len()
        );
        assert_eq!(roundtrip(message.clone()), message);
    }

    #[test]
    fn an_event_needs_no_request_to_belong_to() {
        let message = Message::Event(Event::RepoChanged {
            root: "/srv/app".into(),
        });

        assert_eq!(roundtrip(message.clone()), message);
    }

    #[test]
    fn frames_are_read_back_one_at_a_time_from_a_shared_stream() {
        let mut buffer = Vec::new();
        for id in 1..=3 {
            let message = Message::Request(Envelope {
                id,
                body: Request::ListFiles { root: "/r".into() },
            });
            futures::executor::block_on(write_frame(&mut buffer, &message)).expect("write");
        }

        let mut stream = buffer.as_slice();
        for expected in 1..=3 {
            match futures::executor::block_on(read_frame(&mut stream)).expect("read") {
                Message::Request(envelope) => assert_eq!(envelope.id, expected),
                other => panic!("unexpected frame: {other:?}"),
            }
        }
        assert!(
            matches!(
                futures::executor::block_on(read_frame(&mut stream)),
                Err(FrameError::Closed)
            ),
            "the end of the stream is a clean close, not an error"
        );
    }

    #[test]
    fn an_absurd_length_prefix_is_refused_before_anything_is_allocated() {
        // Four gigabytes claimed by four bytes. Nothing after the header is
        // read, so a hostile stream cannot make the reader hold the buffer.
        let header = u32::MAX.to_le_bytes();

        let refused = futures::executor::block_on(read_frame(&mut header.as_slice()));

        assert!(matches!(refused, Err(FrameError::TooLarge(_))), "{refused:?}");
    }

    #[test]
    fn a_truncated_frame_reads_as_a_closed_connection() {
        let mut buffer = Vec::new();
        futures::executor::block_on(write_frame(
            &mut buffer,
            &Message::Event(Event::RepoChanged { root: "/r".into() }),
        ))
        .expect("write");
        buffer.truncate(buffer.len() - 2);

        let cut = futures::executor::block_on(read_frame(&mut buffer.as_slice()));

        assert!(matches!(cut, Err(FrameError::Closed)), "{cut:?}");
    }
}
