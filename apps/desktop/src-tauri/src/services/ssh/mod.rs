//! Reaching a repository on another machine.
//!
//! Everything here shells out to the system `ssh` rather than speaking the
//! protocol. That is what makes `~/.ssh/config` — `Include`, `Match`,
//! `ProxyJump`, hardware keys, certificate auth, agent forwarding — apply
//! unchanged, and it is the same choice Zed and T3 Code made for the same
//! reason: the user's SSH configuration is the product.

pub mod agent;
pub mod askpass;
pub mod connection;
pub mod host_key;
pub mod hosts;
pub mod target;
pub mod transport;

pub use askpass::Prompt;
pub use hosts::SshHosts;
pub use transport::AgentTransport;
pub use connection::{HostProbe, SshConnection};
pub use host_key::UnknownHostKey;
pub use target::SshTarget;
