//! The half of OnlyDiffs that runs where the repository is.
//!
//! Split out of the desktop app so the remote agent can be built for a Linux
//! host without dragging a webview along with it. Nothing in here knows about
//! windows, IPC, or Groq; the app keeps all of that.

pub mod contract;
pub mod error;
pub mod protocol;
pub mod services;
