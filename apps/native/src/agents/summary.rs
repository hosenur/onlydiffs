//! What the footer says about each agent: a tone for the dot, a short label,
//! and the longer detail shown on hover.

use super::{Agent, claude, codex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// A session is reachable: a message sent now would land.
    Connected,
    /// A session is running but a message would not reach it.
    Unreachable,
    /// Nothing running, or the CLI is not installed.
    Off,
    /// The check itself failed.
    Error,
    /// Not checked yet.
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub tone: Tone,
    pub label: String,
    pub detail: String,
}

fn more(count: usize) -> String {
    if count > 1 {
        format!(" (+{} more)", count - 1)
    } else {
        String::new()
    }
}

pub fn claude(detection: Option<&claude::Detection>) -> Summary {
    use claude::{Activity, Detection, Reach};

    let Some(detection) = detection else {
        return Summary {
            tone: Tone::Pending,
            label: "Claude · checking…".into(),
            detail: String::new(),
        };
    };
    match detection {
        Detection::NotInstalled => Summary {
            tone: Tone::Off,
            label: "Claude · not installed".into(),
            detail: "The `claude` command was not found on PATH or in ~/.local/bin.".into(),
        },
        Detection::Failed(error) => Summary {
            tone: Tone::Error,
            label: "Claude · couldn't check".into(),
            detail: error.clone(),
        },
        Detection::Sessions(sessions) => {
            let ready: Vec<_> = sessions
                .iter()
                .filter(|s| matches!(s.reach, Reach::Ready { .. }))
                .collect();
            if let Some(first) = ready.first() {
                let activity = match first.activity {
                    Activity::Busy => " · working",
                    Activity::Waiting => " · waiting on you",
                    Activity::Idle => " · idle",
                    Activity::Unknown => "",
                };
                let name = first.name.as_deref().unwrap_or("session");
                return Summary {
                    tone: Tone::Connected,
                    label: format!("Claude · connected{activity}{}", more(ready.len())),
                    detail: format!(
                        "Messages go to “{name}” (pid {}) in {}.",
                        first.pid,
                        first.cwd.display()
                    ),
                };
            }
            if sessions.is_empty() {
                return Summary {
                    tone: Tone::Off,
                    label: "Claude · no session".into(),
                    detail: claude::explain_unreachable(sessions),
                };
            }
            Summary {
                tone: Tone::Unreachable,
                label: format!("Claude · running, not reachable{}", more(sessions.len())),
                detail: claude::explain_unreachable(sessions),
            }
        }
    }
}

pub fn codex(detection: Option<&codex::Detection>) -> Summary {
    use codex::{Activity, Detection, Reach};

    let Some(detection) = detection else {
        return Summary {
            tone: Tone::Pending,
            label: "Codex · checking…".into(),
            detail: String::new(),
        };
    };
    match detection {
        Detection::NotInstalled => Summary {
            tone: Tone::Off,
            label: "Codex · not installed".into(),
            detail: "The `codex` command was not found on PATH or in ~/.local/bin.".into(),
        },
        Detection::Sessions(sessions) => {
            let ready: Vec<_> = sessions
                .iter()
                .filter(|s| matches!(s.reach, Reach::Ready { .. }))
                .collect();
            if let Some(first) = ready.first() {
                let activity = match first.activity {
                    Activity::Working => " · working",
                    Activity::WaitingOnYou => " · waiting on you",
                    Activity::Idle => " · idle",
                    Activity::Unknown => "",
                };
                let about = first
                    .preview
                    .as_deref()
                    .map(|preview| format!(" (“{preview}”)"))
                    .unwrap_or_default();
                return Summary {
                    tone: Tone::Connected,
                    label: format!("Codex · connected{activity}{}", more(ready.len())),
                    detail: format!(
                        "Messages go to the session in {} (pid {}){about}.",
                        first.cwd.display(),
                        first.pid
                    ),
                };
            }
            if sessions.is_empty() {
                return Summary {
                    tone: Tone::Off,
                    label: "Codex · no session".into(),
                    detail: codex::explain_unreachable(sessions),
                };
            }
            Summary {
                tone: Tone::Unreachable,
                label: format!("Codex · running, not reachable{}", more(sessions.len())),
                detail: codex::explain_unreachable(sessions),
            }
        }
    }
}

/// What the footer shows for both agents together: only the connected ones
/// when any are, otherwise a single word for why nothing is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overall {
    /// The first check has not finished and nothing is connected yet.
    Checking,
    /// Each connected agent, with where its messages go.
    Connected(Vec<(Agent, String)>),
    /// Something is running but no message would reach it; why, per agent.
    Unreachable(String),
    /// Nothing to send to; why, per agent.
    NoneConnected(String),
}

pub fn overall(agents: &[(Agent, Summary)]) -> Overall {
    let connected: Vec<_> = agents
        .iter()
        .filter(|(_, s)| s.tone == Tone::Connected)
        .map(|(agent, s)| (*agent, s.detail.clone()))
        .collect();
    if !connected.is_empty() {
        return Overall::Connected(connected);
    }
    if agents.iter().any(|(_, s)| s.tone == Tone::Pending) {
        return Overall::Checking;
    }
    let explain = |keep: &dyn Fn(Tone) -> bool| {
        agents
            .iter()
            .filter(|(_, s)| keep(s.tone) && !s.detail.is_empty())
            .map(|(agent, s)| format!("{}: {}", agent.name(), s.detail))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    if agents.iter().any(|(_, s)| s.tone == Tone::Unreachable) {
        Overall::Unreachable(explain(&|tone| tone == Tone::Unreachable))
    } else {
        Overall::NoneConnected(explain(&|_| true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn claude_session(reach: claude::Reach) -> claude::ClaudeSession {
        claude::ClaudeSession {
            pid: 1,
            name: Some("fix parser".into()),
            cwd: PathBuf::from("/repo"),
            activity: claude::Activity::Idle,
            reach,
        }
    }

    #[test]
    fn a_reachable_claude_session_is_connected() {
        let detection = claude::Detection::Sessions(vec![claude_session(claude::Reach::Ready {
            socket: PathBuf::from("/s"),
        })]);
        let summary = claude(Some(&detection));
        assert_eq!(summary.tone, Tone::Connected);
        assert_eq!(summary.label, "Claude · connected · idle");
        assert!(summary.detail.contains("fix parser"));
    }

    #[test]
    fn a_session_without_the_flag_is_unreachable_and_says_how_to_fix_it() {
        let detection =
            claude::Detection::Sessions(vec![claude_session(claude::Reach::MissingFlag)]);
        let summary = claude(Some(&detection));
        assert_eq!(summary.tone, Tone::Unreachable);
        assert!(summary.detail.contains(claude::START_COMMAND));
    }

    #[test]
    fn nothing_running_is_off() {
        let summary = codex(Some(&codex::Detection::Sessions(Vec::new())));
        assert_eq!(summary.tone, Tone::Off);
        assert_eq!(summary.label, "Codex · no session");
    }

    #[test]
    fn unchecked_is_pending() {
        assert_eq!(claude(None).tone, Tone::Pending);
        assert_eq!(codex(None).tone, Tone::Pending);
    }

    fn s(tone: Tone) -> Summary {
        Summary { tone, label: String::new(), detail: format!("{tone:?}") }
    }

    #[test]
    fn connected_hides_the_other_agent() {
        let o = overall(&[(Agent::Claude, s(Tone::Connected)), (Agent::Codex, s(Tone::Off))]);
        assert_eq!(o, Overall::Connected(vec![(Agent::Claude, "Connected".into())]));
    }

    #[test]
    fn both_connected_shows_both() {
        let o = overall(&[(Agent::Claude, s(Tone::Connected)), (Agent::Codex, s(Tone::Connected))]);
        assert!(matches!(o, Overall::Connected(v) if v.len() == 2));
    }

    #[test]
    fn unreachable_wins_over_nothing_running() {
        let o = overall(&[(Agent::Claude, s(Tone::Off)), (Agent::Codex, s(Tone::Unreachable))]);
        assert_eq!(o, Overall::Unreachable("Codex: Unreachable".into()));
    }

    #[test]
    fn nothing_running_is_none_connected() {
        let o = overall(&[(Agent::Claude, s(Tone::Off)), (Agent::Codex, s(Tone::Error))]);
        assert_eq!(o, Overall::NoneConnected("Claude: Off\n\nCodex: Error".into()));
    }

    #[test]
    fn still_checking_says_nothing_yet() {
        let o = overall(&[(Agent::Claude, s(Tone::Pending)), (Agent::Codex, s(Tone::Off))]);
        assert_eq!(o, Overall::Checking);
    }
}
