//! Reads the variables OnlyDiffs documents from the user's login shell.
//!
//! A bundle launched from Finder, Spotlight, or the Dock inherits launchd's
//! environment rather than a terminal's, and that environment is fifteen
//! variables with no `.zshrc` behind it. `GROQ_API_KEY` is therefore unset in a
//! release build on a machine where every terminal has it, which reads as a
//! feature that quietly does nothing. The process environment still wins where
//! it carries the variable — `tauri dev`, the tests, a binary started from a
//! shell — so only the fallback pays for a shell.

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::OnceCell;

/// Long enough for a heavy profile, short enough that a shell blocked on a
/// prompt never holds icon resolution open behind it.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Profiles print — version managers, greetings, update notices. Everything
/// before this marker is theirs, not ours.
const MARKER: &str = "__ONLYDIFFS_ENV__";

static LOGIN_SHELL_ENV: OnceCell<HashMap<String, String>> = OnceCell::const_new();

/// A variable set to whitespace is a variable someone meant to unset; treating
/// it as present would send an empty bearer token to Groq.
fn usable(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// Parses `env` output that a chatty profile has prepended its own lines to.
/// No marker means the shell never reached `env`, which is an empty answer
/// rather than a parse of the profile's own output.
fn parse(stdout: &str) -> HashMap<String, String> {
    let Some((_, printed)) = stdout.split_once(MARKER) else {
        return HashMap::new();
    };
    printed
        // NUL where `env -0` was available, newline where it was not. A value
        // spanning lines survives only in the first form.
        .split(['\0', '\n'])
        .filter_map(|entry| {
            let (name, value) = entry.split_once('=')?;
            (!name.is_empty()).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

#[cfg(unix)]
async fn probe() -> HashMap<String, String> {
    use std::process::Stdio;
    use tokio::process::Command;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
    // `-i` is not redundant with `-l`: zsh reads `.zshrc` only for an
    // interactive shell, and `.zshrc` is where a variable exported for every
    // terminal actually lives. `env -0` keeps values containing newlines
    // intact; the `||` covers a shell whose `env` predates the flag.
    let script = format!("printf '%s' '{MARKER}'; env -0 2>/dev/null || env");
    let output = Command::new(shell)
        .args(["-ilc", &script])
        // An interactive shell that decides to read from a terminal gets EOF
        // instead of the timeout.
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();

    match tokio::time::timeout(PROBE_TIMEOUT, output).await {
        Ok(Ok(output)) => parse(&String::from_utf8_lossy(&output.stdout)),
        Ok(Err(error)) => {
            eprintln!("login shell environment unavailable: {error}");
            HashMap::new()
        }
        Err(_) => {
            eprintln!("login shell environment timed out");
            HashMap::new()
        }
    }
}

#[cfg(not(unix))]
async fn probe() -> HashMap<String, String> {
    // A Windows GUI process already inherits the user's environment.
    HashMap::new()
}

/// The process environment first, then the login shell's, probed once per
/// launch and shared by every later caller.
pub async fn var(name: &str) -> Option<String> {
    if let Some(value) = std::env::var(name).ok().and_then(usable) {
        return Some(value);
    }
    LOGIN_SHELL_ENV
        .get_or_init(probe)
        .await
        .get(name)
        .cloned()
        .and_then(usable)
}

#[cfg(test)]
mod tests {
    use super::{parse, usable, MARKER};

    #[test]
    fn a_chatty_profile_never_becomes_a_variable() {
        let stdout =
            format!("Welcome back!\nnvm: using v22\n{MARKER}GROQ_API_KEY=gsk_key\0PATH=/usr/bin\0");

        let env = parse(&stdout);

        assert_eq!(env.get("GROQ_API_KEY").map(String::as_str), Some("gsk_key"));
        assert_eq!(env.len(), 2, "the greeting is not two more variables");
    }

    #[test]
    fn a_shell_that_never_reached_env_reports_nothing() {
        // Without the marker this line is indistinguishable from a variable,
        // and guessing would hand Groq a profile's error text as a token.
        assert!(parse("command not found: env\n").is_empty());
    }

    #[test]
    fn a_value_containing_an_equals_sign_survives_intact() {
        let env = parse(&format!("{MARKER}GROQ_API_KEY=a=b\0"));

        assert_eq!(env.get("GROQ_API_KEY").map(String::as_str), Some("a=b"));
    }

    #[test]
    fn newline_separated_env_output_is_read_the_same_way() {
        let env = parse(&format!("{MARKER}GROQ_API_KEY=gsk_key\nHOME=/Users/x\n"));

        assert_eq!(env.get("HOME").map(String::as_str), Some("/Users/x"));
    }

    #[test]
    fn a_variable_set_to_whitespace_counts_as_unset() {
        assert_eq!(usable("  ".to_owned()), None);
        assert_eq!(usable(" gsk_key\n".to_owned()), Some("gsk_key".to_owned()));
    }
}
