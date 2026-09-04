//! What the app needs from the machine a repository is on.
//!
//! Every module here runs on whichever machine holds the repository. That is
//! why they live in this crate rather than in the app: the agent compiled for a
//! remote host runs exactly this code, so there is no second diff walk, no
//! second porcelain parser, and no second answer to what `.gitignore` covers.

pub mod attachment;
pub mod claude_channel;
pub mod codex_channel;
pub mod diff;
pub mod file_tree;
// Private on purpose: `repository` is the only caller, and that is what stops a
// later service reaching past the seam and spawning git against a path that may
// not be on this machine.
mod git;
pub mod history;
pub mod icon_scan;
pub mod loopback_http;
pub mod repository;
pub mod watcher;
