//! onlydiffs, the native build: GPUI on top of `onlydiffs-core`.

mod agents;
mod app;
mod assets;
mod engine;
mod projects;
mod explorer;
mod file_icons;
#[rustfmt::skip]
mod file_icons_table;
mod runtime;
mod settings;
mod syntax;
mod settings_view;
mod tabs;
mod text_input;
mod theme;
mod triage;
mod update;
mod workspace;

use std::path::PathBuf;
use std::process::Command;

use gpui::{
    App, Application, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds,
    WindowOptions, actions, prelude::*, px, size,
};
use onlydiffs_core::services::repository::Repository;


actions!(onlydiffs, [Quit]);

/// The repository asked for at launch: the first argument, else
/// `ONLYDIFFS_REPO_PATH`, resolved to the top of its work tree. `None` lets
/// the window restore the project used last, or show the picker.
fn requested_root() -> Option<String> {
    let requested = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("ONLYDIFFS_REPO_PATH").ok())?;
    projects::resolve(&requested).ok()
}

/// The top of the work tree `requested` is in, or `requested` itself when it
/// is not in one.
fn work_tree_top(requested: PathBuf) -> PathBuf {
    Command::new("git")
        .arg("-C")
        .arg(&requested)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()))
        .unwrap_or(requested)
}

/// Test hooks for the agent bridge, usable without the window:
///
/// ```text
/// onlydiffs --agents <dir>                       what is running in <dir>
/// onlydiffs --send claude|codex <dir> <message>  hand one of them a message
/// onlydiffs --other-sessions <dir>               sessions elsewhere it could tie to
/// onlydiffs --triage <dir>                       what triage makes of each change
/// onlydiffs --check-update                      what the updater would find
/// onlydiffs --version                           this build's version
/// ```
///
/// `<dir>` is resolved to the top of its repository, as the window does.
fn run_cli(args: &[String]) -> Option<i32> {
    use agents::{claude, codex};

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(future)
    }
    match args {
        [flag] if flag == "--version" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Some(0)
        }
        [flag] if flag == "--check-update" => Some(update::check_from_cli()),
        [flag, dir] if flag == "--agents" => Some(block_on(async move {
            let root = work_tree_top(PathBuf::from(dir));
            match claude::detect(&root).await {
                claude::Detection::Sessions(sessions) if sessions.is_empty() => {
                    println!("claude: none — {}", claude::explain_unreachable(&[]))
                }
                claude::Detection::Sessions(sessions) => {
                    for s in &sessions {
                        println!(
                            "claude: pid {} {:?} in {} — {:?} — {}",
                            s.pid,
                            s.activity,
                            s.cwd.display(),
                            s.reach,
                            s.name.as_deref().unwrap_or("")
                        );
                    }
                }
                other => println!("claude: {other:?}"),
            }
            match codex::detect(&root).await {
                codex::Detection::Sessions(sessions) if sessions.is_empty() => {
                    println!("codex: none — {}", codex::explain_unreachable(&[]))
                }
                codex::Detection::Sessions(sessions) => {
                    for s in &sessions {
                        println!(
                            "codex: pid {} {:?} in {} — {:?} — {}",
                            s.pid,
                            s.activity,
                            s.cwd.display(),
                            s.reach,
                            s.preview.as_deref().unwrap_or("")
                        );
                    }
                }
                other => println!("codex: {other:?}"),
            }
            0
        })),
        [flag, dir] if flag == "--other-sessions" => Some(block_on(async move {
            let root = work_tree_top(PathBuf::from(dir));
            let ties = agents::other_sessions(&root).await;
            if ties.is_empty() {
                println!("no session in another folder can take messages");
            }
            for tie in ties {
                println!(
                    "{} pid {} in {} — {}",
                    tie.agent.name(),
                    tie.pid,
                    tie.folder(),
                    tie.title.as_deref().unwrap_or("")
                );
            }
            0
        })),
        [flag, dir] if flag == "--triage" => Some(block_on(async move {
            let repo = Repository::local(work_tree_top(PathBuf::from(dir)));
            let diff = match repo.diff().await {
                Ok(diff) => diff,
                Err(error) => {
                    eprintln!("{}", error.message());
                    return 1;
                }
            };
            for file in diff.files {
                if file.binary {
                    println!("{:<52} binary, not judged", file.path);
                    continue;
                }
                let contents = repo
                    .file_contents(&file.path, file.old_path.as_deref(), file.status, file.staged)
                    .await;
                let Ok(contents) = contents else { continue };
                let verdict = triage::judge(
                    &file.path,
                    &contents.old_contents.unwrap_or_default(),
                    &contents.new_contents.unwrap_or_default(),
                );
                println!(
                    "{:<52} {}",
                    format!("{}{}", file.path, if file.staged { " (staged)" } else { "" }),
                    verdict.map_or("no rule applies".to_owned(), |v| v.describe())
                );
            }
            0
        })),
        [flag, agent, dir, message] if flag == "--send" => Some(block_on(async move {
            let root = work_tree_top(PathBuf::from(dir));
            let result = match agent.as_str() {
                "claude" => match claude::detect(&root).await {
                    claude::Detection::Sessions(sessions) => claude::send(&sessions, message)
                        .await
                        .map(|s| format!("sent to Claude pid {}", s.pid)),
                    other => Err(format!("{other:?}")),
                },
                "codex" => match codex::detect(&root).await {
                    codex::Detection::Sessions(sessions) => codex::send(&sessions, message)
                        .await
                        .map(|s| format!("sent to Codex pid {}", s.pid)),
                    other => Err(format!("{other:?}")),
                },
                _ => Err("the agent is `claude` or `codex`".into()),
            };
            match result {
                Ok(done) => {
                    println!("{done}");
                    0
                }
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            }
        })),
        _ => None,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = run_cli(&args) {
        std::process::exit(code);
    }

    let initial = requested_root();

    Application::new().with_assets(assets::Assets).run(move |cx: &mut App| {
        cx.set_global(settings::Settings::load());
        cx.text_system()
            .add_fonts(assets::FONTS.iter().map(|font| std::borrow::Cow::Borrowed(*font)).collect())
            .expect("the bundled fonts load");
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.bind_keys(text_input::bindings());
        cx.bind_keys(app::bindings());
        cx.bind_keys(settings_view::bindings());
        cx.bind_keys(workspace::bindings());
        cx.on_action(|_: &settings_view::OpenSettings, cx| settings_view::open(cx));
        update::init(cx);
        cx.set_menus(vec![Menu {
            name: "onlydiffs".into(),
            items: vec![
                MenuItem::action("Check for Updates…", update::CheckForUpdates),
                MenuItem::action("Settings…", settings_view::OpenSettings),
                MenuItem::separator(),
                MenuItem::action("Quit onlydiffs", Quit),
            ],
        }]);
        // One window; closing it ends the app rather than leaving a dock icon
        // with nothing behind it.
        cx.on_window_closed(|cx| {
            // Closing Settings leaves the app running; closing the main
            // window quits, Settings and all.
            let main_open = cx.windows().iter().any(|w| w.downcast::<workspace::Workspace>().is_some());
            if !main_open {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1100.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("onlydiffs".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| workspace::Workspace::new(initial, window, cx)),
        )
        .expect("the window opens");
        cx.activate(true);
    });
}
