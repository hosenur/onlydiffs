//! Answering ssh's password and passphrase prompts from a GUI.
//!
//! `ssh` asks for secrets on the controlling terminal. A bundle launched from
//! Finder has none, so an `ssh` spawned with an inherited stdin would either
//! read the app's own stdin or block forever with nothing on screen. OpenSSH's
//! answer is `SSH_ASKPASS`: point it at an executable, force it with
//! `SSH_ASKPASS_REQUIRE=force`, give ssh no stdin at all, and every prompt
//! becomes an argv the helper is called with.
//!
//! The helper here is this same binary, re-executed. That is deliberate — a
//! shell script would need a tool that can speak to a unix socket (`nc -U` is
//! not portable), while the app is guaranteed to exist and already knows the
//! protocol. `main()` checks for `ONLYDIFFS_ASKPASS_SOCKET` before Tauri
//! starts; when it is set, the process is a prompt courier and nothing else.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

use crate::error::AppError;

/// The socket the re-executed helper connects back to.
pub const ASKPASS_SOCKET_ENV: &str = "ONLYDIFFS_ASKPASS_SOCKET";

/// A prompt cannot reasonably be longer than this, and a bound keeps a hostile
/// or broken `ssh` from growing the buffer without limit.
const MAX_PROMPT_BYTES: usize = 8 * 1024;
/// Neither can an answer. Passphrases are not megabytes.
const MAX_ANSWER_BYTES: usize = 4 * 1024;

/// One prompt, and the channel that carries the answer back to the helper
/// process that is blocking on it.
pub struct Prompt {
    /// ssh's own words, e.g. `me@host's password:` or
    /// `Enter passphrase for key '/Users/me/.ssh/id_ed25519':`.
    pub text: String,
    reply: oneshot::Sender<Option<String>>,
}

impl Prompt {
    /// Whether ssh is asking for a secret, as opposed to a yes/no question.
    /// Only used to pick the dialog; the answer is passed through either way.
    pub fn is_secret(&self) -> bool {
        let lowered = self.text.to_lowercase();
        lowered.contains("password") || lowered.contains("passphrase")
    }

    pub fn answer(self, value: String) {
        let _ = self.reply.send(Some(value));
    }

    /// Cancelling writes nothing and closes the helper's stream, which ssh
    /// reads as a refusal and gives up on rather than retrying blind.
    pub fn cancel(self) {
        let _ = self.reply.send(None);
    }
}

/// A listening socket, alive for as long as the connection that owns it.
///
/// Dropping it removes the socket file: the directory is a `TempDir` whose
/// destructor runs on drop, so a crashed connection leaves nothing behind for
/// the next one to trip over.
pub struct AskpassServer {
    socket_path: PathBuf,
    _dir: tempfile::TempDir,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for AskpassServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl AskpassServer {
    /// Starts listening, and hands every prompt that arrives to `prompts`.
    ///
    /// The directory is created `0700` by `tempfile`, which matters: anyone who
    /// could connect to this socket could answer an authentication prompt.
    pub fn start(prompts: mpsc::UnboundedSender<Prompt>) -> Result<Self, AppError> {
        let dir = tempfile::Builder::new()
            .prefix("onlydiffs-askpass-")
            .tempdir()
            .map_err(|error| AppError::Ssh(format!("could not create an askpass socket: {error}")))?;
        let socket_path = dir.path().join("askpass.sock");

        let listener = UnixListener::bind(&socket_path)
            .map_err(|error| AppError::Ssh(format!("could not bind the askpass socket: {error}")))?;

        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let prompts = prompts.clone();
                tokio::spawn(async move {
                    serve_one(stream, prompts).await;
                });
            }
        });

        Ok(Self {
            socket_path,
            _dir: dir,
            task,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

/// The server half of the protocol: read one NUL-terminated prompt, write one
/// NUL-terminated answer, close.
async fn serve_one(mut stream: UnixStream, prompts: mpsc::UnboundedSender<Prompt>) {
    let mut buffer = Vec::new();
    let reader = BufReader::new(&mut stream);
    if reader
        .take(MAX_PROMPT_BYTES as u64)
        .read_until(0, &mut buffer)
        .await
        .is_err()
    {
        return;
    }
    if buffer.last() == Some(&0) {
        buffer.pop();
    }
    let text = String::from_utf8_lossy(&buffer).into_owned();

    let (reply, answered) = oneshot::channel();
    if prompts.send(Prompt { text, reply }).is_err() {
        return;
    }

    // A dropped sender is a cancelled prompt, which is the same answer as an
    // explicit cancel: write nothing and let ssh see an empty response.
    if let Ok(Some(answer)) = answered.await {
        let mut payload = answer.into_bytes();
        payload.push(0);
        let _ = stream.write_all(&payload).await;
    }
    let _ = stream.shutdown().await;
}

/// The client half, run in the re-executed helper process.
///
/// Blocking rather than async on purpose: this process exists to make one round
/// trip and exit, and a runtime would be pure overhead in something ssh spawns
/// once per prompt.
pub fn run_helper(socket_path: &str, prompt: &str) -> std::io::Result<String> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream as StdUnixStream;

    let mut stream = StdUnixStream::connect(socket_path)?;
    let mut request = prompt.as_bytes().to_vec();
    request.truncate(MAX_PROMPT_BYTES);
    request.push(0);
    stream.write_all(&request)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut answer = Vec::new();
    std::io::Read::by_ref(&mut stream)
        .take(MAX_ANSWER_BYTES as u64)
        .read_to_end(&mut answer)?;
    if answer.last() == Some(&0) {
        answer.pop();
    }
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

/// The whole of the helper process: connect, relay, print, exit.
///
/// Exit code 1 with no output is how OpenSSH reads "the user cancelled", which
/// is exactly what an empty answer means here.
pub fn helper_main(socket_path: &str) -> ! {
    let prompt = std::env::args().nth(1).unwrap_or_default();
    match run_helper(socket_path, &prompt) {
        Ok(answer) if !answer.is_empty() => {
            println!("{answer}");
            std::process::exit(0)
        }
        _ => std::process::exit(1),
    }
}

/// The path ssh should be pointed at as `SSH_ASKPASS`: this binary.
pub fn helper_binary() -> Result<Arc<PathBuf>, AppError> {
    let exe = std::env::current_exe().map_err(|error| {
        AppError::Ssh(format!("could not locate the OnlyDiffs binary for askpass: {error}"))
    })?;
    Ok(Arc::new(exe))
}

#[cfg(test)]
mod tests {
    use super::{run_helper, AskpassServer, Prompt};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn a_prompt_reaches_the_app_and_the_answer_reaches_ssh() {
        let (tx, mut rx) = mpsc::unbounded_channel::<Prompt>();
        let server = AskpassServer::start(tx).expect("listening");
        let socket = server.socket_path().to_string_lossy().into_owned();

        let helper = tokio::task::spawn_blocking(move || {
            run_helper(&socket, "me@build-box's password:")
        });

        let prompt = rx.recv().await.expect("a prompt arrived");
        assert_eq!(prompt.text, "me@build-box's password:");
        assert!(prompt.is_secret());
        prompt.answer("hunter2".into());

        assert_eq!(helper.await.expect("helper joined").expect("answer"), "hunter2");
    }

    #[tokio::test]
    async fn cancelling_answers_with_nothing_rather_than_hanging() {
        let (tx, mut rx) = mpsc::unbounded_channel::<Prompt>();
        let server = AskpassServer::start(tx).expect("listening");
        let socket = server.socket_path().to_string_lossy().into_owned();

        let helper =
            tokio::task::spawn_blocking(move || run_helper(&socket, "Enter passphrase for key:"));

        rx.recv().await.expect("a prompt arrived").cancel();

        assert_eq!(helper.await.expect("helper joined").expect("closed"), "");
    }

    #[tokio::test]
    async fn a_host_key_question_is_not_treated_as_a_secret() {
        let (tx, mut rx) = mpsc::unbounded_channel::<Prompt>();
        let server = AskpassServer::start(tx).expect("listening");
        let socket = server.socket_path().to_string_lossy().into_owned();

        let helper = tokio::task::spawn_blocking(move || {
            run_helper(&socket, "Are you sure you want to continue connecting (yes/no)?")
        });

        let prompt = rx.recv().await.expect("a prompt arrived");
        assert!(!prompt.is_secret(), "a yes/no question is not a passphrase");
        prompt.answer("yes".into());

        assert_eq!(helper.await.expect("helper joined").expect("answer"), "yes");
    }
}
