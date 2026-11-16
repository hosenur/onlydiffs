//! Keeping the app current, the way Zed does it.
//!
//! A few seconds after launch, and every hour after that, the app asks GitHub
//! for the newest `native-v*` release. When one is newer than this build it
//! downloads the archive, checks its signature, unpacks it and copies it over
//! the running bundle with `rsync --delete`. Then the footer offers
//! "Restart to Update". A running macOS app can be overwritten in place: the
//! process keeps the files it already opened, and the restart picks up the
//! new ones.
//!
//! Native releases are published as GitHub *prereleases*. That keeps
//! `releases/latest` on the Tauri app, whose installed copies read
//! `releases/latest/download/latest.json` and would otherwise find nothing.
//! It is also why this reads the release list from the API instead of
//! following `latest`.
//!
//! The archive is signed with the key the Tauri updater already uses
//! (`plugins.updater.pubkey` in `apps/desktop/src-tauri/tauri.conf.json`,
//! private half in the `TAURI_SIGNING_PRIVATE_KEY` secret), and checked the
//! same way `tauri-plugin-updater` checks it. No signature, no install.
//!
//! Only a copy running from an `.app` bundle updates itself. `cargo run`
//! has nothing to copy over, so it never checks on its own; asking it to
//! from the menu says so.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use base64::Engine as _;
use gpui::{App, AppContext, Context, Entity, Global, Task, actions};
use semver::Version;
use serde::Deserialize;

use crate::runtime;

actions!(update, [CheckForUpdates, RestartToUpdate]);

/// Where the releases are listed. Drafts are left out for anyone not signed
/// in, so a release is only seen once it is published.
const RELEASES_URL: &str = "https://api.github.com/repos/hosenur/onlydiffs/releases?per_page=30";
/// Native releases are tagged `native-v0.1.2`; plain `v0.1.2` is the Tauri app.
const TAG_PREFIX: &str = "native-v";
/// The minisign public key from `tauri.conf.json`, base64 as Tauri stores it.
const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDY1MUE1MDdFRDE5NTU4NTUKUldSVldKWFJmbEFhWmJwT0F0cGZPci9UdlFBM3ArZGFGWnlQcE5RT2pCS2FZS0dFSnNsMlpHa1oK";

/// Late enough to stay out of the way of the first paint and its loaders.
const FIRST_CHECK: Duration = Duration::from_secs(5);
/// Zed's interval for stable builds.
const POLL_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// How long the answer to a check someone asked for stays in the footer.
const NOTICE_FOR: Duration = Duration::from_secs(8);
/// How long one address gets to answer before the next is tried. On the
/// user's network one of GitHub's asset addresses never answers, and without
/// this a check waited out the kernel's ~75 s before trying the next one.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

/// The archive for this machine, e.g. `onlydiffs-native-aarch64.app.tar.gz`.
fn asset_name() -> String {
    format!("onlydiffs-native-{}.app.tar.gz", std::env::consts::ARCH)
}

/// The version this build reports. `ONLYDIFFS_PRETEND_VERSION` stands in for
/// it, so the whole update can be tried from an older-looking build.
pub fn current_version() -> Version {
    std::env::var("ONLYDIFFS_PRETEND_VERSION")
        .ok()
        .and_then(|v| Version::parse(&v).ok())
        .unwrap_or_else(|| Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver"))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    Downloading { version: Version, progress: Option<f32> },
    Installing { version: Version },
    /// Installed over the bundle; a restart runs it.
    Updated { version: Version },
    /// The answer to a check someone asked for. Automatic checks keep quiet.
    UpToDate,
    Errored(String),
}

pub struct Updater {
    status: Status,
    /// The running bundle, or `None` when not running from one.
    bundle: Option<PathBuf>,
    /// The check in flight; dropping it cancels it.
    check: Option<Task<()>>,
    clear_notice: Option<Task<()>>,
    _poll: Option<Task<()>>,
}

struct GlobalUpdater(Entity<Updater>);

impl Global for GlobalUpdater {}

/// Starts the updater: checks shortly after launch and hourly, when running
/// from a bundle.
pub fn init(cx: &mut App) {
    let bundle = cx.app_path().ok().filter(|path| path.extension().is_some_and(|ext| ext == "app"));
    let updater = cx.new(|cx: &mut Context<Updater>| {
        let poll = bundle.is_some().then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(FIRST_CHECK).await;
                loop {
                    if this.update(cx, |this, cx| this.check(false, cx)).is_err() {
                        break;
                    }
                    cx.background_executor().timer(POLL_INTERVAL).await;
                }
            })
        });
        Updater { status: Status::Idle, bundle, check: None, clear_notice: None, _poll: poll }
    });
    cx.set_global(GlobalUpdater(updater));
    cx.on_action(|_: &CheckForUpdates, cx| {
        if let Some(updater) = entity(cx) {
            updater.update(cx, |this, cx| this.check(true, cx));
        }
    });
    cx.on_action(|_: &RestartToUpdate, cx| restart(cx));
}

pub fn entity(cx: &App) -> Option<Entity<Updater>> {
    cx.try_global::<GlobalUpdater>().map(|global| global.0.clone())
}

pub fn status(cx: &App) -> Status {
    entity(cx).map_or(Status::Idle, |updater| updater.read(cx).status.clone())
}

/// Relaunches into the installed update.
pub fn restart(cx: &mut App) {
    if matches!(status(cx), Status::Updated { .. }) {
        cx.restart();
    }
}

impl Updater {
    fn set_status(&mut self, status: Status, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    /// A notice that clears itself, for the answer to a check someone asked for.
    fn notice(&mut self, status: Status, cx: &mut Context<Self>) {
        self.set_status(status.clone(), cx);
        self.clear_notice = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTICE_FOR).await;
            this.update(cx, |this, cx| {
                if this.status == status {
                    this.set_status(Status::Idle, cx);
                }
            })
            .ok();
        }));
    }

    /// Looks for a newer release and installs it. `manual` is a check someone
    /// asked for, which reports its outcome; an automatic one only speaks up
    /// once there is something to restart into.
    fn check(&mut self, manual: bool, cx: &mut Context<Self>) {
        match self.status {
            // Already at it, or already done until the restart.
            Status::Checking
            | Status::Downloading { .. }
            | Status::Installing { .. }
            | Status::Updated { .. } => return,
            _ => {}
        }
        let Some(bundle) = self.bundle.clone() else {
            if manual {
                self.notice(
                    Status::Errored("Updates install into the app bundle; this copy is not running from one".into()),
                    cx,
                );
            }
            return;
        };
        self.set_status(if manual { Status::Checking } else { Status::Idle }, cx);
        self.check = Some(cx.spawn(async move |this, cx| {
            let result = Self::update(this.clone(), bundle, cx).await;
            this.update(cx, |this, cx| {
                this.check = None;
                match result {
                    Ok(Some(version)) => this.set_status(Status::Updated { version }, cx),
                    Ok(None) if manual => this.notice(Status::UpToDate, cx),
                    Err(error) if manual => this.notice(Status::Errored(format!("{error:#}")), cx),
                    // Offline is the normal state of a laptop; an automatic
                    // check that fails tries again in an hour.
                    _ => this.set_status(Status::Idle, cx),
                }
            })
            .ok();
        }));
    }

    /// The whole update. `Ok(None)` when this build is the newest.
    async fn update(this: gpui::WeakEntity<Self>, bundle: PathBuf, cx: &mut gpui::AsyncApp) -> Result<Option<Version>> {
        let client = client()?;
        let found = {
            let client = client.clone();
            runtime::run(async move { find_release(&client).await }).await??
        };
        let Some(release) = found.filter(|r| r.version > current_version()) else {
            return Ok(None);
        };
        let version = release.version.clone();
        this.update(cx, |this, cx| {
            this.set_status(Status::Downloading { version: version.clone(), progress: None }, cx)
        })?;

        let dir = InstallerDir::new()?;
        let archive = dir.path().join(asset_name());
        let (progress_tx, mut progress_rx) = tokio::sync::watch::channel(None::<f32>);
        let download = {
            let (client, release, archive) = (client.clone(), release.clone(), archive.clone());
            runtime::run(async move { download(&client, &release, &archive, progress_tx).await })
        };
        let progress = {
            let this = this.clone();
            let mut cx = cx.clone();
            async move {
                while progress_rx.changed().await.is_ok() {
                    let progress = *progress_rx.borrow();
                    this.update(&mut cx, |this, cx| {
                        if let Status::Downloading { progress: current, .. } = &mut this.status {
                            *current = progress;
                            cx.notify();
                        }
                    })
                    .ok();
                }
            }
        };
        let (downloaded, ()) = futures_join(download, progress).await;
        let signature = downloaded??;

        this.update(cx, |this, cx| this.set_status(Status::Installing { version: version.clone() }, cx))?;
        let expected = version.clone();
        runtime::run(async move {
            let bytes = tokio::fs::read(&archive).await.context("reading the download")?;
            verify(&bytes, &signature)?;
            install(&dir, &archive, &expected, &bundle).await
        })
        .await??;
        Ok(Some(version))
    }
}

/// Awaits both futures. Small enough not to pull in `futures` for it.
async fn futures_join<A: std::future::Future, B: std::future::Future>(a: A, b: B) -> (A::Output, B::Output) {
    let mut a = std::pin::pin!(a);
    let mut b = std::pin::pin!(b);
    let mut out_a = None;
    let mut out_b = None;
    std::future::poll_fn(|cx| {
        if out_a.is_none() {
            if let std::task::Poll::Ready(v) = a.as_mut().poll(cx) {
                out_a = Some(v);
            }
        }
        if out_b.is_none() {
            if let std::task::Poll::Ready(v) = b.as_mut().poll(cx) {
                out_b = Some(v);
            }
        }
        if out_a.is_some() && out_b.is_some() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
    (out_a.unwrap(), out_b.unwrap())
}

fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        // GitHub's API refuses requests without one.
        .user_agent(concat!("onlydiffs-native/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("could not start the HTTP client")
}

#[derive(Debug, Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Debug, Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

/// A release with an archive for this machine.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: Version,
    pub archive_url: String,
    pub signature_url: String,
}

/// The newest published native release with an archive and a signature for
/// this machine.
pub async fn find_release(client: &reqwest::Client) -> Result<Option<Release>> {
    let response = client
        .get(RELEASES_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("could not reach GitHub")?;
    ensure!(response.status().is_success(), "GitHub answered {}", response.status());
    let releases: Vec<ApiRelease> = response.json().await.context("GitHub's release list did not parse")?;
    Ok(newest(&releases, &asset_name()))
}

fn newest(releases: &[ApiRelease], asset: &str) -> Option<Release> {
    let signature = format!("{asset}.sig");
    releases
        .iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = Version::parse(r.tag_name.strip_prefix(TAG_PREFIX)?).ok()?;
            let url = |name: &str| r.assets.iter().find(|a| a.name == name).map(|a| a.browser_download_url.clone());
            Some(Release { version, archive_url: url(asset)?, signature_url: url(&signature)? })
        })
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// Downloads the archive to `to`, reporting progress, and returns the
/// signature that came with it.
async fn download(
    client: &reqwest::Client,
    release: &Release,
    to: &Path,
    progress: tokio::sync::watch::Sender<Option<f32>>,
) -> Result<String> {
    use tokio::io::AsyncWriteExt as _;

    let signature = client.get(&release.signature_url).send().await.context("could not fetch the signature")?;
    ensure!(signature.status().is_success(), "the signature answered {}", signature.status());
    let signature = signature.text().await.context("could not read the signature")?;

    let mut response = client.get(&release.archive_url).send().await.context("could not start the download")?;
    ensure!(response.status().is_success(), "the download answered {}", response.status());
    let total = response.content_length().filter(|&n| n > 0);
    let mut file = tokio::fs::File::create(to).await.context("could not save the download")?;
    let mut done = 0u64;
    while let Some(chunk) = response.chunk().await.context("the download stopped")? {
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        if let Some(total) = total {
            progress.send_replace(Some((done as f32 / total as f32).min(1.0)));
        }
    }
    file.flush().await?;
    Ok(signature)
}

/// Checks `data` against a `.sig` from `tauri signer sign`, as
/// `tauri-plugin-updater` does: both the key and the signature are minisign
/// text, wrapped in base64.
fn verify(data: &[u8], signature: &str) -> Result<()> {
    verify_with(PUBLIC_KEY, data, signature)
}

fn verify_with(public_key: &str, data: &[u8], signature: &str) -> Result<()> {
    let decode = |text: &str| -> Result<String> {
        let bytes = base64::engine::general_purpose::STANDARD.decode(text.trim())?;
        Ok(String::from_utf8(bytes)?)
    };
    let key = minisign_verify::PublicKey::decode(&decode(public_key).context("the public key is not base64")?)
        .map_err(|e| anyhow!("the public key did not parse: {e}"))?;
    let signature = minisign_verify::Signature::decode(&decode(signature).context("the signature is not base64")?)
        .map_err(|e| anyhow!("the signature did not parse: {e}"))?;
    key.verify(data, &signature, true).map_err(|_| anyhow!("the download's signature does not match"))
}

/// Unpacks the archive, checks the app inside reports `expected`, and copies
/// it over the running bundle.
async fn install(dir: &InstallerDir, archive: &Path, expected: &Version, bundle: &Path) -> Result<()> {
    let unpacked = dir.path().join("unpacked");
    tokio::fs::create_dir_all(&unpacked).await?;
    run(tokio::process::Command::new("tar").arg("-xzf").arg(archive).arg("-C").arg(&unpacked), "unpack the update")
        .await?;

    let mut apps = Vec::new();
    let mut entries = tokio::fs::read_dir(&unpacked).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.path().extension().is_some_and(|ext| ext == "app") {
            apps.push(entry.path());
        }
    }
    let [app] = apps.as_slice() else { bail!("the update holds {} apps, not one", apps.len()) };

    // The ad-hoc seal catches a damaged copy; the version catches a release
    // whose tag and build disagree, which would otherwise update forever.
    run(tokio::process::Command::new("codesign").args(["--verify", "--deep", "--strict"]).arg(app), "verify the update")
        .await?;
    let reported = run(
        tokio::process::Command::new(app.join("Contents/MacOS/onlydiffs")).arg("--version"),
        "run the update",
    )
    .await?;
    ensure!(
        reported.trim() == expected.to_string(),
        "the update reports version {}, not {expected}",
        reported.trim()
    );

    // The trailing slash copies the bundle's contents rather than the bundle
    // into itself.
    let mut from = app.as_os_str().to_owned();
    from.push("/");
    run(
        tokio::process::Command::new("rsync").args(["-a", "--delete", "--exclude", "Icon?"]).arg(from).arg(bundle),
        "install the update",
    )
    .await?;
    Ok(())
}

async fn run(command: &mut tokio::process::Command, what: &str) -> Result<String> {
    let output = command.output().await.with_context(|| format!("could not {what}"))?;
    ensure!(
        output.status.success(),
        "could not {what}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// A scratch folder for one update, removed when dropped.
struct InstallerDir(PathBuf);

impl InstallerDir {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!("onlydiffs-update-{}", std::process::id()));
        // Left over from an update that never finished.
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).context("could not make a folder for the update")?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for InstallerDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

/// `onlydiffs --check-update`: what the updater would find, without
/// installing anything.
pub fn check_from_cli() -> i32 {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    rt.block_on(async {
        let found = match client() {
            Ok(client) => find_release(&client).await,
            Err(error) => Err(error),
        };
        let current = current_version();
        match found {
            Ok(Some(release)) if release.version > current => {
                println!("{} is available (this is {current})", release.version);
                println!("  {}", release.archive_url);
                0
            }
            Ok(Some(release)) => {
                println!("up to date: this is {current}, the newest release is {}", release.version);
                0
            }
            Ok(None) => {
                println!("no {TAG_PREFIX}* release has a signed {} yet (this is {current})", asset_name());
                0
            }
            Err(error) => {
                eprintln!("{error:#}");
                1
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, draft: bool, assets: &[&str]) -> ApiRelease {
        ApiRelease {
            tag_name: tag.into(),
            draft,
            assets: assets
                .iter()
                .map(|name| ApiAsset { name: (*name).into(), browser_download_url: format!("https://x/{tag}/{name}") })
                .collect(),
        }
    }

    const A: &str = "onlydiffs-native-aarch64.app.tar.gz";
    const S: &str = "onlydiffs-native-aarch64.app.tar.gz.sig";

    #[test]
    fn the_newest_signed_native_release_wins() {
        let releases = [
            release("v0.1.20", false, &[A, S]), // the Tauri app
            release("native-v0.1.9", false, &[A, S]),
            release("native-v0.1.10", false, &[A, S]),
            release("native-v0.2.0", true, &[A, S]), // a draft
            release("native-v0.1.11", false, &[A]),  // unsigned
            release("native-v0.1.12", false, &["onlydiffs-native-x86_64.app.tar.gz", S]),
        ];
        let found = newest(&releases, A).expect("one qualifies");
        assert_eq!(found.version, Version::new(0, 1, 10));
        assert_eq!(found.signature_url, format!("https://x/native-v0.1.10/{S}"));
    }

    #[test]
    fn nothing_qualifies_without_native_releases() {
        assert_eq!(newest(&[release("v0.1.20", false, &[A, S])], A), None);
    }

    /// Signs the way `tauri signer sign` does, then checks it the way the app
    /// does.
    #[test]
    fn a_tauri_style_signature_verifies_and_a_tampered_file_does_not() {
        let b64 = |text: String| base64::engine::general_purpose::STANDARD.encode(text);
        let minisign::KeyPair { pk, sk } = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let data = b"the archive".to_vec();
        let signature =
            minisign::sign(Some(&pk), &sk, std::io::Cursor::new(&data), Some("trusted"), None).unwrap();
        let key = b64(pk.to_box().unwrap().to_string());
        let signature = b64(signature.to_string());

        verify_with(&key, &data, &signature).expect("the signature matches");
        assert!(verify_with(&key, b"something else", &signature).is_err());
    }

    /// The install half, against an archive from `scripts/bundle.sh`:
    ///
    /// ```text
    /// ./scripts/bundle.sh
    /// ONLYDIFFS_TEST_ARCHIVE=target/release/bundle/onlydiffs-native-aarch64.app.tar.gz \
    ///   cargo test installs_over_a_bundle -- --ignored
    /// ```
    #[test]
    #[ignore = "needs an archive from scripts/bundle.sh"]
    fn installs_over_a_bundle() {
        let archive = PathBuf::from(std::env::var("ONLYDIFFS_TEST_ARCHIVE").expect("ONLYDIFFS_TEST_ARCHIVE"));
        let version = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let dir = InstallerDir::new().unwrap();
            // A stand-in for the running copy: an old binary and a stray file
            // the new bundle does not have, which `--delete` should remove.
            let running = dir.path().join("running/onlydiffs-native.app");
            std::fs::create_dir_all(running.join("Contents/MacOS")).unwrap();
            std::fs::write(running.join("Contents/MacOS/onlydiffs"), "old").unwrap();
            std::fs::write(running.join("Contents/stray"), "gone after").unwrap();

            install(&dir, &archive, &version, &running).await.expect("installs");
            assert!(!running.join("Contents/stray").exists());
            let reported = std::process::Command::new(running.join("Contents/MacOS/onlydiffs"))
                .arg("--version")
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&reported.stdout).trim(), version.to_string());

            let wrong = Version::new(9, 9, 9);
            let error = install(&dir, &archive, &wrong, &running).await.unwrap_err();
            assert!(format!("{error:#}").contains("not 9.9.9"), "{error:#}");
        });
    }

    #[test]
    fn the_shipped_key_parses() {
        let text = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(PUBLIC_KEY).unwrap()).unwrap();
        minisign_verify::PublicKey::decode(&text).expect("the key from tauri.conf.json parses");
    }
}
