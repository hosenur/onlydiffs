//! Resolving what the user typed into the host `ssh` will actually dial.
//!
//! `ssh -G` is the only correct way to do this. `~/.ssh/config` supports
//! `Include`, `Match exec`, wildcard `Host` patterns, per-host `User`, `Port`,
//! `HostName`, `ProxyJump` and token expansion in `ControlPath` — reproducing
//! any of that here would be a second, worse implementation that disagrees with
//! the ssh the app then shells out to. Asking ssh means the two never differ.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::error::AppError;

/// How long `ssh -G` gets. It performs no network I/O — it only reads config —
/// so anything approaching this is a `Match exec` block that hangs.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(10);

/// A resolved SSH destination: what the user typed, plus what ssh made of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    /// Exactly what the user typed — `build-box`, `me@10.0.0.4`, an alias from
    /// their config. This is what gets handed back to `ssh`, so that config
    /// keeps applying; the resolved fields below are for display and identity.
    pub alias: String,
    pub hostname: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// The user's own effective `ControlPath` for this host, tokens already
    /// expanded by ssh. `None` when they have not configured one.
    pub control_path: Option<PathBuf>,
    /// The user's `known_hosts` files, in the order ssh consults them. Read
    /// from the config rather than assumed: `UserKnownHostsFile` is routinely
    /// repointed per-host, and checking a file ssh will not read would report
    /// an unknown host as trusted.
    pub user_known_hosts: Vec<PathBuf>,
    /// The system-wide ones. Consulted, never written: they belong to the
    /// machine's administrator, not to whoever is running this app.
    pub global_known_hosts: Vec<PathBuf>,
}

impl SshTarget {
    /// A stable identity for this destination, for keying connections and
    /// remembered projects. Deliberately the resolved triple rather than the
    /// alias: two aliases for one machine are one machine.
    pub fn connection_key(&self) -> String {
        format!(
            "{}@{}:{}",
            self.user.as_deref().unwrap_or(""),
            self.hostname,
            self.port.unwrap_or(22)
        )
    }

    /// Every `known_hosts` file ssh will read for this host, in its order:
    /// the user's first, then the machine's.
    pub fn known_hosts_files(&self) -> impl Iterator<Item = &PathBuf> {
        self.user_known_hosts.iter().chain(&self.global_known_hosts)
    }

    /// Where an approved key should be recorded.
    ///
    /// The first user file that is a real file rather than a sink. `/dev/null`
    /// is a legitimate thing to *read* — it finds nothing — but writing an
    /// approval into it would report success and trust nothing, which is the
    /// worst of both.
    pub fn known_hosts_for_writing(&self) -> Option<&PathBuf> {
        self.user_known_hosts
            .iter()
            .find(|path| !is_sink(path))
    }

    /// `user@host`, or just the host when ssh will supply the user.
    pub fn destination(&self) -> String {
        match &self.user {
            Some(user) => format!("{user}@{}", self.hostname),
            None => self.hostname.clone(),
        }
    }
}

/// Parses `ssh -G` output: one `keyword value` per line, lowercase keyword,
/// first occurrence winning — which is ssh's own precedence rule.
pub(crate) fn parse_resolved(alias: &str, stdout: &str) -> SshTarget {
    let mut hostname = None;
    let mut user = None;
    let mut port = None;
    let mut control_path = None;
    let mut user_known_hosts: Vec<PathBuf> = Vec::new();
    let mut global_known_hosts: Vec<PathBuf> = Vec::new();

    for line in stdout.lines() {
        let Some((keyword, value)) = line.trim().split_once(char::is_whitespace) else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        // First wins: ssh prints the effective value first for each keyword.
        match keyword {
            "hostname" if hostname.is_none() => hostname = Some(value.to_owned()),
            "user" if user.is_none() => user = Some(value.to_owned()),
            "port" if port.is_none() => port = value.parse::<u16>().ok(),
            "controlpath" if control_path.is_none() && value != "none" => {
                control_path = Some(PathBuf::from(value));
            }
            // Both keywords take a space-separated list, and `none` is a real
            // value meaning "consult nothing". They are kept apart rather than
            // concatenated in the order ssh prints them: `ssh -G` sorts its
            // output alphabetically, so global comes first there while ssh
            // itself reads the user's files first — and only the user's are
            // ever ours to write to.
            "userknownhostsfile" => collect_paths(value, &mut user_known_hosts),
            "globalknownhostsfile" => collect_paths(value, &mut global_known_hosts),
            _ => {}
        }
    }

    SshTarget {
        // A destination ssh could not resolve is still worth dialling as
        // typed; ssh will produce the better error than we could.
        hostname: hostname.unwrap_or_else(|| strip_user(alias).to_owned()),
        alias: alias.to_owned(),
        user,
        port,
        control_path,
        user_known_hosts,
        global_known_hosts,
    }
}

fn collect_paths(value: &str, into: &mut Vec<PathBuf>) {
    for entry in value.split_whitespace().filter(|entry| *entry != "none") {
        let path = expand_home(entry);
        if !into.contains(&path) {
            into.push(path);
        }
    }
}

/// A path that discards what is written to it. Only `/dev/null` in practice,
/// but naming it keeps the intent legible at the call site.
fn is_sink(path: &Path) -> bool {
    path == Path::new("/dev/null")
}

/// `ssh -G` prints paths as configured, so a leading `~` arrives unexpanded.
fn expand_home(value: &str) -> PathBuf {
    let Some(rest) = value.strip_prefix("~/") else {
        return PathBuf::from(value);
    };
    match dirs::home_dir() {
        Some(home) => home.join(rest),
        None => PathBuf::from(value),
    }
}

fn strip_user(alias: &str) -> &str {
    alias.rsplit_once('@').map(|(_, host)| host).unwrap_or(alias)
}

/// Rejects a destination that would be read as an option rather than a host.
/// `ssh` takes flags before the destination, so a target beginning with `-`
/// would silently become one.
pub fn validate_target(alias: &str) -> Result<&str, AppError> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Err(AppError::Ssh("Enter an SSH host.".into()));
    }
    if alias.starts_with('-') {
        return Err(AppError::Ssh(format!(
            "\"{alias}\" starts with a dash, which ssh would read as an option rather than a host."
        )));
    }
    if alias.contains(|c: char| c.is_whitespace()) {
        return Err(AppError::Ssh(format!(
            "\"{alias}\" contains whitespace; an SSH host cannot."
        )));
    }
    Ok(alias)
}

/// Asks ssh what this destination resolves to.
pub async fn resolve(alias: &str, extra_args: &[String]) -> Result<SshTarget, AppError> {
    let alias = validate_target(alias)?;

    let output = Command::new("ssh")
        .args(extra_args)
        .arg("-G")
        .arg(alias)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();

    let output = match tokio::time::timeout(RESOLVE_TIMEOUT, output).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(AppError::Ssh(format!(
                "could not run ssh: {error}. Is OpenSSH installed?"
            )))
        }
        Err(_) => {
            return Err(AppError::Ssh(format!(
                "ssh -G {alias} did not finish within {}s — a Match exec block in your SSH config is probably hanging.",
                RESOLVE_TIMEOUT.as_secs()
            )))
        }
    };

    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Ssh(format!(
            "ssh could not resolve \"{alias}\": {}",
            detail.trim()
        )));
    }

    Ok(parse_resolved(alias, &String::from_utf8_lossy(&output.stdout)))
}

#[cfg(test)]
mod tests {
    use super::{parse_resolved, validate_target};
    use std::path::PathBuf;

    // Alphabetical, the way `ssh -G` actually prints it: global before user.
const SAMPLE: &str = "controlpath /Users/me/.ssh/cm-deploy@10.0.0.4:2222
globalknownhostsfile /etc/ssh/ssh_known_hosts none
hostname 10.0.0.4
port 2222
user deploy
userknownhostsfile /tmp/kh /tmp/kh2
forwardagent no
";

    #[test]
    fn the_resolved_host_beats_the_alias_that_was_typed() {
        let target = parse_resolved("build-box", SAMPLE);

        assert_eq!(target.alias, "build-box");
        assert_eq!(target.hostname, "10.0.0.4");
        assert_eq!(target.user.as_deref(), Some("deploy"));
        assert_eq!(target.port, Some(2222));
    }

    #[test]
    fn a_configured_control_path_is_picked_up_for_reuse() {
        let target = parse_resolved("build-box", SAMPLE);

        assert_eq!(
            target.control_path,
            Some(PathBuf::from("/Users/me/.ssh/cm-deploy@10.0.0.4:2222"))
        );
    }

    #[test]
    fn control_path_none_is_no_control_path() {
        // ssh prints the literal string "none" when multiplexing is off, and
        // treating that as a socket would produce a path nothing listens on.
        let target = parse_resolved("box", "hostname box\ncontrolpath none\n");

        assert_eq!(target.control_path, None);
    }

    #[test]
    fn the_first_value_wins_the_way_ssh_reads_its_own_config() {
        let target = parse_resolved("box", "hostname first.example\nhostname second.example\n");

        assert_eq!(target.hostname, "first.example");
    }

    #[test]
    fn an_unresolvable_alias_still_yields_a_host_to_dial() {
        let target = parse_resolved("me@box.example", "");

        assert_eq!(target.hostname, "box.example");
        assert_eq!(target.destination(), "box.example");
    }

    #[test]
    fn two_aliases_for_one_machine_share_a_connection_key() {
        let a = parse_resolved("build", "hostname 10.0.0.4\nuser deploy\nport 22\n");
        let b = parse_resolved("build-box", "hostname 10.0.0.4\nuser deploy\nport 22\n");

        assert_eq!(a.connection_key(), b.connection_key());
    }

    #[test]
    fn the_users_known_hosts_are_read_before_the_machines() {
        // `ssh -G` sorts its output, so the input here has global first — the
        // reading order must still be the user's files first.
        let target = parse_resolved("build-box", SAMPLE);

        assert_eq!(
            target.known_hosts_files().collect::<Vec<_>>(),
            vec![
                &PathBuf::from("/tmp/kh"),
                &PathBuf::from("/tmp/kh2"),
                &PathBuf::from("/etc/ssh/ssh_known_hosts"),
            ],
            "`none` is not a file, and the user's files come first"
        );
    }

    #[test]
    fn an_approved_key_is_recorded_in_the_users_file_never_the_machines() {
        let target = parse_resolved("build-box", SAMPLE);

        assert_eq!(
            target.known_hosts_for_writing(),
            Some(&PathBuf::from("/tmp/kh"))
        );
    }

    #[test]
    fn a_known_hosts_that_discards_writes_is_not_somewhere_to_record_trust() {
        let target = parse_resolved(
            "box",
            "globalknownhostsfile /etc/ssh/ssh_known_hosts\nhostname box\nuserknownhostsfile /dev/null\n",
        );

        // Reading it is fine — it simply finds nothing.
        assert_eq!(target.known_hosts_files().count(), 2);
        assert_eq!(target.known_hosts_for_writing(), None);
    }

    #[test]
    fn a_host_that_consults_no_known_hosts_file_collects_none() {
        let target = parse_resolved("box", "hostname box\nuserknownhostsfile none\n");

        assert_eq!(target.known_hosts_files().count(), 0);
    }

    #[test]
    fn a_target_that_would_be_read_as_an_option_is_refused() {
        assert!(validate_target("-oProxyCommand=curl evil.example").is_err());
        assert!(validate_target("   ").is_err());
        assert!(validate_target("box name").is_err());
        assert_eq!(validate_target("  build-box  ").expect("valid"), "build-box");
    }
}

/// Splits a command line the way a shell would, honouring quotes.
///
/// People paste the command they already use, and that command routinely has a
/// quoted path in it — `ssh -i "~/keys/my key" box`. Splitting on whitespace
/// would turn that into two arguments and an error nobody could act on.
pub(crate) fn tokenize(input: &str) -> Result<Vec<String>, AppError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                // Inside double quotes a backslash escapes the next character,
                // which is how a quote ends up in a path.
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), c) => current.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                // An empty quoted string is still an argument.
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (None, c) => {
                current.push(c);
                started = true;
            }
        }
    }

    if quote.is_some() {
        return Err(AppError::Ssh(
            "That command has an unclosed quote.".into(),
        ));
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

/// The `ssh` options that take a value, so the token after one is not mistaken
/// for the destination. From ssh(1); anything not listed here is a flag.
const VALUED_FLAGS: &[char] = &[
    'B', 'b', 'c', 'D', 'E', 'e', 'F', 'I', 'i', 'J', 'L', 'l', 'm', 'O', 'o', 'P', 'p', 'Q', 'R',
    'S', 'W', 'w',
];

/// What the user typed, split into a destination and the options around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshCommand {
    /// The destination ssh would dial: `build-box`, `me@10.0.0.4`.
    pub destination: String,
    /// Every option that came with it, in order, ready to hand back to `ssh`.
    pub args: Vec<String>,
}

/// Reads an `ssh` command the way the user already writes it.
///
/// `ssh user@example -p 2222`, `ssh -i ~/.ssh/work -J jump me@box`, or just
/// `box`. Options may come before or after the host, because real `ssh`
/// permutes them and a parser that did not would reject the exact form most
/// people write.
/// Taking the whole command rather than an alias means a host that needs a port
/// or an identity is one paste rather than an edit to `~/.ssh/config` — and the
/// options are kept and replayed on every later connection, so they are not a
/// one-time thing that works and then quietly stops.
pub fn parse_ssh_command(input: &str) -> Result<SshCommand, AppError> {
    let tokens = tokenize(input.trim())?;
    if tokens.is_empty() {
        return Err(AppError::Ssh("Enter an SSH host or command.".into()));
    }

    let mut rest = tokens.as_slice();
    // The leading `ssh` is optional: people paste the whole command, and people
    // type just the host.
    if rest[0] == "ssh" {
        rest = &rest[1..];
    }

    let mut args = Vec::new();
    let mut destination: Option<String> = None;
    let mut index = 0;

    while index < rest.len() {
        let token = &rest[index];

        let Some(flag) = token.strip_prefix('-') else {
            if destination.is_some() {
                // The *second* bare token is a command to run on the host. This
                // app runs its own, so accepting one would silently drop it.
                return Err(AppError::Ssh(format!(
                    "Remove `{}` — OnlyDiffs runs its own commands on the host.",
                    rest[index..].join(" ")
                )));
            }
            destination = Some(token.clone());
            index += 1;
            continue;
        };

        if flag.is_empty() {
            return Err(AppError::Ssh("`-` is not an SSH option.".into()));
        }
        args.push(token.clone());

        // `-p 2222` takes the next token; `-p2222` carries its own value.
        let first = flag.chars().next().unwrap_or_default();
        if VALUED_FLAGS.contains(&first) && flag.chars().count() == 1 {
            index += 1;
            let value = rest.get(index).ok_or_else(|| {
                AppError::Ssh(format!("`-{first}` needs a value after it."))
            })?;
            args.push(value.clone());
        }
        index += 1;
    }

    let destination = destination
        .ok_or_else(|| AppError::Ssh("That command names no host to connect to.".into()))?;
    validate_target(&destination)?;

    Ok(SshCommand { destination, args })
}

#[cfg(test)]
mod command_tests {
    use super::{parse_ssh_command, tokenize};

    #[test]
    fn a_bare_host_is_a_command_with_no_options() {
        let parsed = parse_ssh_command("build-box").expect("parsed");

        assert_eq!(parsed.destination, "build-box");
        assert!(parsed.args.is_empty());
    }

    #[test]
    fn the_command_people_actually_paste_is_understood() {
        let parsed = parse_ssh_command("ssh user@example -p 2222").expect("parsed");

        assert_eq!(parsed.destination, "user@example");
        assert_eq!(parsed.args, vec!["-p", "2222"]);
    }

    #[test]
    fn options_before_the_host_are_kept_in_order() {
        let parsed =
            parse_ssh_command("ssh -i ~/.ssh/work -J jump.example -p 2222 me@box").expect("parsed");

        assert_eq!(parsed.destination, "me@box");
        assert_eq!(
            parsed.args,
            vec!["-i", "~/.ssh/work", "-J", "jump.example", "-p", "2222"]
        );
    }

    #[test]
    fn a_flag_carrying_its_own_value_is_one_token() {
        // `-p2222` is as valid as `-p 2222`, and consuming the next token for
        // it would swallow the host.
        let parsed = parse_ssh_command("ssh -p2222 me@box").expect("parsed");

        assert_eq!(parsed.destination, "me@box");
        assert_eq!(parsed.args, vec!["-p2222"]);
    }

    #[test]
    fn boolean_flags_do_not_swallow_the_host() {
        let parsed = parse_ssh_command("ssh -A -4 box").expect("parsed");

        assert_eq!(parsed.destination, "box");
        assert_eq!(parsed.args, vec!["-A", "-4"]);
    }

    #[test]
    fn a_quoted_path_survives_as_one_argument() {
        let parsed = parse_ssh_command(r#"ssh -i "~/my keys/id_ed25519" box"#).expect("parsed");

        assert_eq!(parsed.args, vec!["-i", "~/my keys/id_ed25519"]);
        assert_eq!(parsed.destination, "box");
    }

    #[test]
    fn a_trailing_remote_command_is_refused_rather_than_dropped() {
        // Accepting it would look like it worked and then never run.
        let refused = parse_ssh_command("ssh box tmux attach").expect_err("refused");

        assert!(refused.message().contains("tmux attach"), "{refused:?}");
    }

    #[test]
    fn a_command_with_no_host_is_refused() {
        assert!(parse_ssh_command("ssh -p 2222").is_err());
        assert!(parse_ssh_command("ssh").is_err());
        assert!(parse_ssh_command("   ").is_err());
    }

    #[test]
    fn options_after_the_host_are_kept_too_because_ssh_permutes_them() {
        // Verified against the real thing: `ssh -G user@example -p 2222`
        // resolves port 2222, so refusing this form would reject the exact
        // command most people paste.
        let parsed = parse_ssh_command("ssh me@box -A -p 2222 -i key").expect("parsed");

        assert_eq!(parsed.destination, "me@box");
        assert_eq!(parsed.args, vec!["-A", "-p", "2222", "-i", "key"]);
    }

    #[test]
    fn a_value_flag_with_nothing_after_it_says_so() {
        let refused = parse_ssh_command("ssh box -p").expect_err("refused");

        assert!(refused.message().contains("needs a value"), "{refused:?}");
    }

    #[test]
    fn an_unclosed_quote_is_reported_rather_than_guessed_at() {
        assert!(tokenize(r#"ssh -i "unclosed box"#).is_err());
    }

    #[test]
    fn a_host_that_would_be_read_as_an_option_is_still_refused() {
        // The destination goes back to `ssh` verbatim, so this is the same
        // check as the one on a typed alias.
        assert!(parse_ssh_command("ssh -- -oProxyCommand=curl").is_err());
    }
}
