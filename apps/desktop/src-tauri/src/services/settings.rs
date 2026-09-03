//! The app's own settings, in `~/.onlydiffs/config.json`.
//!
//! What separates this file from `projects.json` beside it: that one is a
//! history the app writes for itself, this one is only ever what someone chose
//! deliberately. Nothing is written here that the user did not set.
//!
//! The Groq key lives here because the environment cannot carry it on its own.
//! A bundle launched from Finder has no shell behind it, so `GROQ_API_KEY`
//! exported from `.zshrc` is simply absent — and even where the fallback in
//! `shell_env` finds it, editing a dotfile is a poor way to change a key. The
//! environment still works and still wins nothing: a key set here is the one
//! the app uses, because it is the only one the user can see from inside it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::contract::{AppSettings, GroqKeySource, SshHostEntry};
use crate::error::AppError;
use crate::services::shell_env;

const CONFIG_FILE: &str = "config.json";
const CONFIG_VERSION: u32 = 1;

/// The stored form. Every field is optional and defaulted, so a config written
/// by a newer build loses only the settings this one has no name for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    groq_api_key: Option<String>,
    /// SSH destinations the user has added, in the order they added them.
    ///
    /// A host's real name, port, user and keys still live in `~/.ssh/config`
    /// where ssh reads them; what is stored here is only what the user typed
    /// that ssh would not otherwise know — the options from the command they
    /// pasted. A host that needs nothing extra stores nothing extra.
    #[serde(default, deserialize_with = "hosts", skip_serializing_if = "Vec::is_empty")]
    ssh_hosts: Vec<SshHostEntry>,
}

/// Reads a host list written by any version of this app.
///
/// An earlier build stored bare alias strings. Failing to parse them would not
/// merely lose the hosts — the whole `Config` would fail, fall back to the
/// default, and take the user's Groq key with it. Accepting both shapes costs
/// twelve lines and removes that entire class of upgrade bug.
fn hosts<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<SshHostEntry>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Alias(String),
        Entry(SshHostEntry),
    }

    Ok(Vec::<Stored>::deserialize(deserializer)?
        .into_iter()
        .map(|stored| match stored {
            Stored::Alias(alias) => SshHostEntry {
                alias,
                args: Vec::new(),
            },
            Stored::Entry(entry) => entry,
        })
        .collect())
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            groq_api_key: None,
            ssh_hosts: Vec::new(),
        }
    }
}

/// `gsk_abcd…WxYz`. Enough to tell one key from another at a glance, never
/// enough to be one: this is what the settings page shows in place of the key,
/// which itself never crosses to the renderer.
fn hint(key: &str) -> String {
    let characters: Vec<char> = key.chars().collect();
    if characters.len() < 12 {
        return "•".repeat(8);
    }
    let head: String = characters.iter().take(4).collect();
    let tail: String = characters[characters.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// A value someone cleared by selecting it and deleting it is a value they
/// meant to remove, not an empty key to send to Groq.
fn usable(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// A missing or unreadable file just means "nothing configured yet". A config
/// from a version this build does not know is left alone rather than
/// overwritten, so downgrading and relaunching does not wipe it.
fn read_config(config_path: &Path) -> Config {
    std::fs::read_to_string(config_path)
        .ok()
        .and_then(|body| serde_json::from_str::<Config>(&body).ok())
        .filter(|config| config.version <= CONFIG_VERSION)
        .unwrap_or_default()
}

/// The file holds an API key, so it is owner-only. `mode` on the open covers
/// the file this call creates; `set_permissions` covers one an earlier build
/// left at the default.
#[cfg(unix)]
fn write_private(config_path: &Path, body: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(config_path)?;
    file.write_all(body.as_bytes())?;
    std::fs::set_permissions(config_path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn write_private(config_path: &Path, body: &str) -> std::io::Result<()> {
    std::fs::write(config_path, body)
}

pub struct Settings {
    config_path: PathBuf,
    config: Mutex<Config>,
}

impl Settings {
    pub fn new(state_dir: PathBuf) -> Self {
        let config_path = state_dir.join(CONFIG_FILE);
        Self {
            config: Mutex::new(read_config(&config_path)),
            config_path,
        }
    }

    /// The same state directory the recents list uses, so `ONLYDIFFS_STATE_DIR`
    /// moves everything the app owns at once.
    pub fn from_env() -> Self {
        Self::new(super::state_dir())
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    fn stored_groq_key(&self) -> Option<String> {
        self.config
            .lock()
            .expect("settings lock")
            .groq_api_key
            .as_deref()
            .and_then(usable)
    }

    /// The key the Groq features will actually use, and where it came from.
    ///
    /// Config before environment: a key typed into the settings page is the
    /// only one of the two the user can see from inside the app, so it has to
    /// beat a `GROQ_API_KEY` that may have been exported months ago and
    /// forgotten. Someone who wants the environment back clears the field.
    pub async fn groq_key(&self) -> Option<(String, GroqKeySource)> {
        if let Some(key) = self.stored_groq_key() {
            return Some((key, GroqKeySource::Config));
        }
        let key = shell_env::var("GROQ_API_KEY").await?;
        Some((key, GroqKeySource::Environment))
    }

    /// Stores a key, or clears the stored one when given nothing. Clearing
    /// hands the app back to `GROQ_API_KEY` where that is set, which is why it
    /// removes the field rather than storing an empty string.
    pub fn set_groq_key(&self, key: Option<&str>) -> Result<(), AppError> {
        let mut config = self.config.lock().expect("settings lock");
        config.version = CONFIG_VERSION;
        config.groq_api_key = key.and_then(usable);
        let config = config.clone();
        self.write(&config)
    }

    /// The SSH destinations the user has added.
    pub fn ssh_hosts(&self) -> Vec<SshHostEntry> {
        self.config.lock().expect("settings lock").ssh_hosts.clone()
    }

    /// The options a host is dialled with, or none if it is not remembered.
    pub fn ssh_args(&self, alias: &str) -> Vec<String> {
        self.config
            .lock()
            .expect("settings lock")
            .ssh_hosts
            .iter()
            .find(|host| host.alias == alias)
            .map(|host| host.args.clone())
            .unwrap_or_default()
    }

    /// Adds a host, or replaces the options on one already there.
    ///
    /// Replacing rather than refusing: re-adding a host with a corrected port
    /// is how anyone fixes a typo, and making that a no-op would leave the old
    /// options in place with nothing on screen to say why it still fails.
    ///
    /// Order is preserved rather than sorted — it is the order the user added
    /// them in, and a list that reshuffles itself is one nobody can build a
    /// habit around.
    pub fn add_ssh_host(&self, entry: SshHostEntry) -> Result<(), AppError> {
        if usable(&entry.alias).is_none() {
            return Err(AppError::Ssh("Enter an SSH host.".into()));
        }
        let config = {
            let mut config = self.config.lock().expect("settings lock");
            config.version = CONFIG_VERSION;
            match config
                .ssh_hosts
                .iter_mut()
                .find(|host| host.alias == entry.alias)
            {
                Some(existing) => existing.args = entry.args,
                None => config.ssh_hosts.push(entry),
            }
            config.clone()
        };
        self.write(&config)
    }

    pub fn forget_ssh_host(&self, alias: &str) -> Result<(), AppError> {
        let config = {
            let mut config = self.config.lock().expect("settings lock");
            config.version = CONFIG_VERSION;
            config.ssh_hosts.retain(|existing| existing.alias != alias);
            config.clone()
        };
        self.write(&config)
    }

    /// What the settings page renders. The key itself is deliberately absent:
    /// the page can report that one is set, and replace it, without the value
    /// ever entering a webview that also renders untrusted diff text.
    pub async fn snapshot(&self) -> AppSettings {
        let resolved = self.groq_key().await;
        AppSettings {
            groq_api_key_hint: resolved.as_ref().map(|(key, _)| hint(key)),
            groq_key_source: resolved.map_or(GroqKeySource::None, |(_, source)| source),
            config_path: self.config_path.to_string_lossy().into_owned(),
            ssh_hosts: self.ssh_hosts(),
        }
    }

    fn write(&self, config: &Config) -> Result<(), AppError> {
        let body = serde_json::to_string_pretty(config)
            .map_err(|error| AppError::Settings(format!("failed to encode settings: {error}")))?;
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::Settings(format!("failed to create {}: {error}", parent.display()))
            })?;
        }
        // Unlike the recents file, a failure here is worth reporting: someone
        // just pressed Save and needs to know the key did not land.
        write_private(&self.config_path, &body).map_err(|error| {
            AppError::Settings(format!(
                "failed to write {}: {error}",
                self.config_path.display()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{hint, read_config, Settings};
    use crate::contract::{GroqKeySource, SshHostEntry};

    fn settings() -> (tempfile::TempDir, Settings) {
        let state = tempfile::TempDir::new().expect("temp state");
        let settings = Settings::new(state.path().to_path_buf());
        (state, settings)
    }

    #[test]
    fn a_hint_identifies_a_key_without_carrying_it() {
        let masked = hint("gsk_0123456789abcdefWxYz");

        assert_eq!(masked, "gsk_…WxYz");
        assert!(!masked.contains("0123456789"));
    }

    #[test]
    fn a_key_too_short_to_mask_is_hidden_outright() {
        assert_eq!(hint("short"), "••••••••");
    }

    #[test]
    fn a_saved_key_survives_a_relaunch() {
        let (state, settings) = settings();
        settings
            .set_groq_key(Some("gsk_0123456789abcdefWxYz"))
            .expect("save");

        let reopened = Settings::new(state.path().to_path_buf());

        assert_eq!(
            reopened.stored_groq_key().as_deref(),
            Some("gsk_0123456789abcdefWxYz")
        );
    }

    #[tokio::test]
    async fn a_saved_key_beats_the_environment() {
        // Whatever `GROQ_API_KEY` holds on the machine running this — set on a
        // developer's laptop, unset in CI — the stored one is the answer.
        let (_state, settings) = settings();
        settings
            .set_groq_key(Some("gsk_0123456789abcdefWxYz"))
            .expect("save");

        let resolved = settings.groq_key().await.expect("a key is configured");

        assert_eq!(resolved.0, "gsk_0123456789abcdefWxYz");
        assert_eq!(resolved.1, GroqKeySource::Config);
    }

    #[test]
    fn clearing_the_key_removes_the_field_rather_than_blanking_it() {
        let (state, settings) = settings();
        settings
            .set_groq_key(Some("gsk_0123456789abcdefWxYz"))
            .expect("save");

        settings.set_groq_key(None).expect("clear");

        assert!(settings.stored_groq_key().is_none());
        let body = std::fs::read_to_string(state.path().join("config.json")).expect("read config");
        // An empty string left behind would be sent to Groq as a bearer token.
        assert!(
            !body.contains("groqApiKey"),
            "cleared key should leave no field"
        );
    }

    #[test]
    fn whitespace_is_not_a_key() {
        let (_state, settings) = settings();

        settings.set_groq_key(Some("   ")).expect("save");

        assert!(settings.stored_groq_key().is_none());
    }

    #[test]
    fn a_surrounding_paste_is_trimmed_before_it_is_stored() {
        let (_state, settings) = settings();

        settings
            .set_groq_key(Some("  gsk_0123456789abcdefWxYz\n"))
            .expect("save");

        assert_eq!(
            settings.stored_groq_key().as_deref(),
            Some("gsk_0123456789abcdefWxYz")
        );
    }

    fn host(alias: &str, args: &[&str]) -> SshHostEntry {
        SshHostEntry {
            alias: alias.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }

    #[test]
    fn ssh_hosts_are_kept_in_the_order_they_were_added() {
        let (state, settings) = settings();
        settings.add_ssh_host(host("build-box", &[])).expect("add");
        settings
            .add_ssh_host(host("me@10.0.0.4", &["-p", "2222"]))
            .expect("add");

        let stored = settings.ssh_hosts();
        assert_eq!(stored[0].alias, "build-box");
        assert_eq!(stored[1].args, vec!["-p", "2222"]);
        assert_eq!(Settings::new(state.path().to_path_buf()).ssh_hosts().len(), 2);
    }

    #[test]
    fn re_adding_a_host_corrects_its_options_rather_than_doing_nothing() {
        // Fixing a typo in the port is the whole reason someone re-adds a host.
        let (_state, settings) = settings();
        settings.add_ssh_host(host("box", &["-p", "2222"])).expect("add");

        settings.add_ssh_host(host("box", &["-p", "2022"])).expect("re-add");

        assert_eq!(settings.ssh_hosts().len(), 1);
        assert_eq!(settings.ssh_args("box"), vec!["-p", "2022"]);
    }

    #[test]
    fn forgetting_a_host_leaves_the_others_in_place() {
        let (_state, settings) = settings();
        settings.add_ssh_host(host("a", &[])).expect("add");
        settings.add_ssh_host(host("b", &[])).expect("add");

        settings.forget_ssh_host("a").expect("forget");

        assert_eq!(settings.ssh_hosts().len(), 1);
        assert_eq!(settings.ssh_hosts()[0].alias, "b");
    }

    #[test]
    fn a_host_list_written_as_bare_aliases_still_reads() {
        // An earlier build stored strings. Failing to parse them would fail the
        // whole config and take the Groq key with it.
        let state = tempfile::TempDir::new().expect("temp state");
        std::fs::write(
            state.path().join("config.json"),
            r#"{"version":1,"groqApiKey":"gsk_0123456789abcdefWxYz","sshHosts":["build-box","me@box"]}"#,
        )
        .expect("write");

        let settings = Settings::new(state.path().to_path_buf());

        assert_eq!(settings.ssh_hosts().len(), 2);
        assert_eq!(settings.ssh_hosts()[0].alias, "build-box");
        assert!(settings.ssh_hosts()[0].args.is_empty());
        assert!(settings.stored_groq_key().is_some(), "the key must survive");
    }

    #[test]
    fn a_mixed_host_list_reads_both_shapes() {
        let state = tempfile::TempDir::new().expect("temp state");
        std::fs::write(
            state.path().join("config.json"),
            r#"{"version":1,"sshHosts":["plain",{"alias":"box","args":["-p","2222"]}]}"#,
        )
        .expect("write");

        let settings = Settings::new(state.path().to_path_buf());

        assert_eq!(settings.ssh_args("plain"), Vec::<String>::new());
        assert_eq!(settings.ssh_args("box"), vec!["-p", "2222"]);
    }

    #[test]
    fn a_settings_file_written_before_ssh_existed_still_reads() {
        let state = tempfile::TempDir::new().expect("temp state");
        std::fs::write(
            state.path().join("config.json"),
            r#"{"version":1,"groqApiKey":"gsk_0123456789abcdefWxYz"}"#,
        )
        .expect("write");

        let settings = Settings::new(state.path().to_path_buf());

        assert!(settings.ssh_hosts().is_empty());
        assert!(settings.stored_groq_key().is_some(), "the key still reads");
    }

    #[test]
    fn a_corrupt_config_reads_as_no_settings_rather_than_failing() {
        let state = tempfile::TempDir::new().expect("temp state");
        let path = state.path().join("config.json");
        std::fs::write(&path, "{ not json").expect("write");

        assert!(read_config(&path).groq_api_key.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn the_config_file_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let (state, settings) = settings();
        settings
            .set_groq_key(Some("gsk_0123456789abcdefWxYz"))
            .expect("save");

        let mode = std::fs::metadata(state.path().join("config.json"))
            .expect("stat config")
            .permissions()
            .mode();

        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_config_left_world_readable_by_an_earlier_build_is_tightened() {
        use std::os::unix::fs::PermissionsExt;

        let (state, settings) = settings();
        let path = state.path().join("config.json");
        std::fs::write(&path, "{}").expect("write");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("loosen");

        settings
            .set_groq_key(Some("gsk_0123456789abcdefWxYz"))
            .expect("save");

        let mode = std::fs::metadata(&path)
            .expect("stat config")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
