//! The window: changed files on the left, open files as tabs on the right.
//!
//! Phase 1 of the GPUI build. Deliberately plain: no animation, no effects,
//! system font for the chrome and a monospace font for code.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext, Context, Entity, FocusHandle, Focusable, FontWeight, Hsla, InteractiveElement, IntoElement,
    KeyBinding, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, Task, Window, actions, div, prelude::FluentBuilder, px,
    relative, svg, uniform_list, anchored, deferred, point, Corner, ScrollHandle, UniformListScrollHandle,
};
use onlydiffs_core::contract::{ChangeStatus, FileChange, RepoDiff};
use onlydiffs_core::services::repository::Repository;
use onlydiffs_core::services::watcher::RepoWatcher;

use crate::agents::summary::{self, Summary, Tone};
use crate::agents::{self, Agent, LineReference, Tie, claude, codex};
use crate::engine::{self, FileDiff, Line, Row, RowKind};
use crate::runtime;
use crate::tabs::Tabs;
use crate::theme::Palette;
use crate::text_input::{InputColors, TextInput};

actions!(composer, [SendMessage, CloseComposer]);
actions!(agent_menu, [CloseAgentMenu]);
actions!(context_menu, [CloseContextMenu]);
actions!(
    project_panel,
    [SelectNext, SelectPrevious, CollapseSelected, ExpandSelected, CollapseAll, ExpandAll, OpenSelected]
);
actions!(
    pane,
    [
        CloseTab,
        CloseOtherTabs,
        StageActiveFile,
        NextTab,
        PreviousTab,
        ActivateTab1,
        ActivateTab2,
        ActivateTab3,
        ActivateTab4,
        ActivateTab5,
        ActivateTab6,
        ActivateTab7,
        ActivateTab8,
        ActivateTab9,
        ActivateLastTab,
    ]
);

const COMPOSER_CONTEXT: &str = "Composer";
const PANE_CONTEXT: &str = "Pane";
const AGENT_MENU_CONTEXT: &str = "AgentMenu";
const CONTEXT_MENU_CONTEXT: &str = "ContextMenu";
const TREE_CONTEXT: &str = "ProjectPanel";
/// How long "Sent to …" stays in the footer.
const SENT_NOTE: Duration = Duration::from_secs(6);

/// The window's key bindings, registered once at startup. The tab keys are
/// Zed's.
pub fn bindings() -> Vec<KeyBinding> {
    let pane = Some(PANE_CONTEXT);
    vec![
        KeyBinding::new("enter", SendMessage, Some(COMPOSER_CONTEXT)),
        KeyBinding::new("escape", CloseComposer, Some(COMPOSER_CONTEXT)),
        KeyBinding::new("escape", CloseAgentMenu, Some(AGENT_MENU_CONTEXT)),
        KeyBinding::new("escape", CloseContextMenu, Some(CONTEXT_MENU_CONTEXT)),
        // The tree's keys are Zed's project panel's.
        KeyBinding::new("down", SelectNext, Some(TREE_CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(TREE_CONTEXT)),
        KeyBinding::new("left", CollapseSelected, Some(TREE_CONTEXT)),
        KeyBinding::new("right", ExpandSelected, Some(TREE_CONTEXT)),
        KeyBinding::new("cmd-left", CollapseAll, Some(TREE_CONTEXT)),
        KeyBinding::new("cmd-right", ExpandAll, Some(TREE_CONTEXT)),
        KeyBinding::new("space", OpenSelected, Some(TREE_CONTEXT)),
        KeyBinding::new("enter", OpenSelected, Some(TREE_CONTEXT)),
        KeyBinding::new("cmd-w", CloseTab, pane),
        KeyBinding::new("cmd-enter", StageActiveFile, pane),
        KeyBinding::new("alt-cmd-t", CloseOtherTabs, pane),
        KeyBinding::new("cmd-}", NextTab, pane),
        KeyBinding::new("alt-cmd-right", NextTab, pane),
        KeyBinding::new("ctrl-tab", NextTab, pane),
        KeyBinding::new("cmd-{", PreviousTab, pane),
        KeyBinding::new("alt-cmd-left", PreviousTab, pane),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, pane),
        KeyBinding::new("ctrl-1", ActivateTab1, pane),
        KeyBinding::new("ctrl-2", ActivateTab2, pane),
        KeyBinding::new("ctrl-3", ActivateTab3, pane),
        KeyBinding::new("ctrl-4", ActivateTab4, pane),
        KeyBinding::new("ctrl-5", ActivateTab5, pane),
        KeyBinding::new("ctrl-6", ActivateTab6, pane),
        KeyBinding::new("ctrl-7", ActivateTab7, pane),
        KeyBinding::new("ctrl-8", ActivateTab8, pane),
        KeyBinding::new("ctrl-9", ActivateTab9, pane),
        KeyBinding::new("ctrl-0", ActivateLastTab, pane),
    ]
}

/// The right-click menu on a file.
struct ContextMenu {
    file_id: String,
    /// Where the click was, in window coordinates: the menu opens there.
    at: gpui::Point<gpui::Pixels>,
}

/// Which list the agent menu shows.
enum MenuPage {
    Main,
    /// Sessions in other folders to tie this window to.
    Pick(Load<Vec<Tie>>),
}

/// A file open in a tab.
/// The colours for both sides of a diff: removed lines are coloured as the
/// old file reads, the rest as the new file does.
#[derive(Clone, Default)]
struct Syntax {
    old: Option<Arc<crate::syntax::Highlights>>,
    new: Option<Arc<crate::syntax::Highlights>>,
}

impl Syntax {
    /// The spans for one diff line, if its side was highlighted.
    fn spans(&self, line: &Line) -> Option<&[crate::syntax::Span]> {
        let (side, number) = match line.kind {
            RowKind::Removed => (&self.old, line.old_number),
            _ => (&self.new, line.new_number),
        };
        let lines = &side.as_ref()?.lines;
        lines.get(number?.checked_sub(1)? as usize).map(Vec::as_slice)
    }
}

struct Opened {
    diff: Load<Arc<FileDiff>>,
    syntax: Syntax,
    /// Each tab keeps its own scroll position.
    scroll: UniformListScrollHandle,
    /// Which load `diff` is waiting for, so a slow one that a refresh has
    /// overtaken cannot land on top of the newer one.
    generation: u64,
}

/// The message being written about a clicked line.
struct Composer {
    reference: LineReference,
    input: Entity<TextInput>,
    agent: Agent,
    sending: bool,
    error: Option<SharedString>,
}

const ROW_HEIGHT: f32 = 22.;
const SIDEBAR_WIDTH: f32 = 300.;
const NUMBER_WIDTH: f32 = 48.;
const FOOTER_HEIGHT: f32 = 26.;
const TAB_HEIGHT: f32 = 32.;
const TAB_MAX_WIDTH: f32 = 240.;
/// A tab for a file opened whole from the tree, not a change: `file:<path>`.
const PLAIN: &str = "file:";
/// Bigger unchanged files are not opened; nobody reads them in a viewer.
const PLAIN_LIMIT: u64 = 4 * 1024 * 1024;
/// How far each level of the tree is indented, as in Zed.
const INDENT: f32 = 20.;
/// How often the agent check runs while the window is open. A check costs one
/// `ps`, one `claude agents --json` and a few local socket connects.
const AGENT_POLL: Duration = Duration::from_secs(3);

/// Something being fetched from the repository.
enum Load<T> {
    Loading,
    Ready(T),
    Failed(SharedString),
}

pub struct OnlyDiffs {
    repo: Repository,
    diff: Load<RepoDiff>,
    tabs: Tabs,
    /// Every file in the repository, as a tree, and as a set for lookups.
    tree: crate::explorer::Dir,
    all_paths: HashSet<String>,
    /// Open folders in the tree, by path.
    expanded: HashSet<String>,
    /// The sidebar's rows for the open folders; rebuilt when either changes.
    rows: Vec<crate::explorer::Row>,
    /// The tree's highlighted row, by path, for the arrow keys.
    explorer_selection: Option<String>,
    explorer_focus: FocusHandle,
    explorer_scroll: UniformListScrollHandle,
    /// The tab last revealed in the tree, so a refresh does not reopen
    /// folders the user closed around it.
    revealed: Option<String>,
    /// What each tab shows, by `FileChange::id`.
    opened: HashMap<String, Opened>,
    generation: u64,
    tab_scroll: ScrollHandle,
    /// Held by the window whenever the composer is not, so the tab keys work.
    focus: FocusHandle,
    palette: Palette,
    /// The latest agent checks; `None` until the first one finishes.
    claude: Option<claude::Detection>,
    codex: Option<codex::Detection>,
    /// A check is in flight. The timer and window focus both start checks,
    /// and this keeps them from piling up when one is slow.
    checking_agents: bool,
    composer: Option<Composer>,
    /// The agent last sent to, offered first next time while it is connected.
    preferred_agent: Agent,
    /// "Sent to …", shown in the footer for a few seconds after a send.
    sent_note: Option<SharedString>,
    /// The footer note reports a failure.
    note_is_error: bool,
    /// Files being staged right now, by `FileChange::id`: their buttons wait,
    /// and a second press does nothing until the first lands.
    staging: HashSet<String>,
    /// The triage rules' verdict on each changed file they cover, by
    /// `FileChange::id`.
    triage: HashMap<String, crate::triage::Verdict>,
    /// Bumped on every refresh; a triage pass for an older list stops.
    triage_generation: u64,
    /// The menu over the footer's agent status is showing.
    agent_menu: bool,
    agent_menu_page: MenuPage,
    /// A session in another folder that messages go to instead.
    tie: Option<Tie>,
    /// The pointer is over the status that opens the menu. A click there
    /// toggles the menu, so the menu's "clicked elsewhere" must ignore it.
    over_agent_status: bool,
    agent_menu_focus: FocusHandle,
    context_menu: Option<ContextMenu>,
    context_menu_focus: FocusHandle,
    note_generation: u64,
    _activation: Subscription,
    _agent_poll: Task<()>,
    /// On screen; hidden projects pause their agent checks.
    visible: bool,
    /// Redraw when the settings or the system appearance change.
    _settings: Subscription,
    /// Repaints the footer as an update downloads and installs.
    _updater: Option<Subscription>,
    _appearance: Subscription,
    /// A read of the changed files is running; `reload_again` asks for one
    /// more after it, so a change made during a read is never missed and
    /// reads never pile up.
    reloading: bool,
    reload_again: bool,
    /// Watches the repository and pokes `_watch_events` when it changes.
    _watcher: RepoWatcher,
    _watch_events: Task<()>,
}

impl OnlyDiffs {
    pub fn new(repo: Repository, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Coming back to the window is when a stale status would be noticed —
        // the user has just started or quit a session in a terminal.
        // Files change underneath the window — an agent writing, a commit in
        // a terminal. The watcher filters out ignored paths and waits for a
        // burst of writes to settle, then this reads the changes again.
        let watcher = RepoWatcher::new();
        let (changed, mut on_change) = tokio::sync::mpsc::unbounded_channel::<()>();
        watcher.watch(repo.root().to_path_buf(), move || {
            changed.send(()).ok();
        });
        let watch_events = cx.spawn(async move |this, cx| {
            while on_change.recv().await.is_some() {
                // Several notices may be queued; one read covers them all.
                while on_change.try_recv().is_ok() {}
                if this.update(cx, |this, cx| this.reload(cx)).is_err() {
                    break;
                }
            }
        });
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                // Also read again on coming back, in case the watch could not
                // be set up — it fails quietly.
                this.reload(cx);
                this.check_agents(cx);
            }
        });
        let agent_poll = cx.spawn(async move |this, cx| {
            loop {
                if this.update(cx, |this, cx| this.check_agents(cx)).is_err() {
                    break;
                }
                cx.background_executor().timer(AGENT_POLL).await;
            }
        });
        let mut this = Self {
            repo,
            diff: Load::Loading,
            tabs: Tabs::default(),
            tree: crate::explorer::Dir::default(),
            all_paths: HashSet::new(),
            expanded: HashSet::new(),
            rows: Vec::new(),
            explorer_selection: None,
            explorer_focus: cx.focus_handle(),
            explorer_scroll: UniformListScrollHandle::new(),
            revealed: None,
            opened: HashMap::new(),
            generation: 0,
            tab_scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            palette: Palette::current(window.appearance(), cx),
            claude: None,
            codex: None,
            checking_agents: false,
            composer: None,
            preferred_agent: Agent::Claude,
            sent_note: None,
            note_is_error: false,
            staging: HashSet::new(),
            triage: HashMap::new(),
            triage_generation: 0,
            agent_menu: false,
            agent_menu_page: MenuPage::Main,
            tie: None,
            over_agent_status: false,
            agent_menu_focus: cx.focus_handle(),
            context_menu: None,
            context_menu_focus: cx.focus_handle(),
            note_generation: 0,
            _activation: activation,
            _agent_poll: agent_poll,
            visible: true,
            _settings: cx.observe_global::<crate::settings::Settings>(|_, cx| cx.notify()),
            _updater: crate::update::entity(cx).map(|updater| cx.observe(&updater, |_, _, cx| cx.notify())),
            _appearance: cx.observe_window_appearance(window, |_, _, cx| cx.notify()),
            reloading: false,
            reload_again: false,
            _watcher: watcher,
            _watch_events: watch_events,
        };
        window.focus(&this.focus);
        this.reload(cx);
        this
    }

    /// Asks which Claude Code and Codex sessions are running in the repository.
    /// Shown or hidden by the project rail. A hidden project stops asking
    /// about agents; shown again, it catches up at once.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if visible {
            self.reload(cx);
            self.check_agents(cx);
        }
    }

    /// Gives this project's window keyboard focus.
    pub fn focus(&self, window: &mut Window, _: &mut Context<Self>) {
        window.focus(&self.focus);
    }

    fn check_agents(&mut self, cx: &mut Context<Self>) {
        if self.checking_agents || !self.visible {
            return;
        }
        self.checking_agents = true;
        let root = self.repo.root().to_path_buf();
        let tie = self.tie.clone();
        cx.spawn(async move |this, cx| {
            let checked_tie = tie.clone();
            let result = runtime::run(async move {
                let alive = async {
                    match &checked_tie {
                        Some(tie) => agents::tie_alive(tie).await,
                        None => true,
                    }
                };
                tokio::join!(claude::detect(&root), codex::detect(&root), alive)
            })
            .await;
            this.update(cx, |this, cx| {
                this.checking_agents = false;
                let result = result.map(|(claude, codex, alive)| {
                    // Let go of a tied session that has quit — unless the
                    // user tied to another one while this check ran.
                    if !alive && this.tie == tie
                        && let Some(gone) = this.tie.take()
                    {
                        this.show_sent_note(
                            format!("The {} session in {} ended.", gone.agent.name(), gone.folder()).into(),
                            cx,
                        );
                    }
                    (claude, codex)
                });
                match result {
                    Ok((claude, codex)) => {
                        this.claude = Some(claude);
                        this.codex = Some(codex);
                    }
                    Err(error) => {
                        this.claude = Some(claude::Detection::Failed(error.to_string()));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn summary(&self, agent: Agent) -> Summary {
        match agent {
            Agent::Claude => summary::claude(self.claude.as_ref()),
            Agent::Codex => summary::codex(self.codex.as_ref()),
        }
    }

    fn is_connected(&self, agent: Agent) -> bool {
        self.tied(agent).is_some() || self.summary(agent).tone == Tone::Connected
    }

    /// The tied session, when it is `agent`'s.
    fn tied(&self, agent: Agent) -> Option<&Tie> {
        self.tie.as_ref().filter(|tie| tie.agent == agent)
    }

    /// The agent a new message goes to: the tied one, else the last one used
    /// while it is still connected, else whichever is connected, else the
    /// last one used.
    fn pick_agent(&self) -> Agent {
        if let Some(tie) = &self.tie {
            return tie.agent;
        }
        if self.is_connected(self.preferred_agent) {
            return self.preferred_agent;
        }
        Agent::ALL
            .into_iter()
            .find(|agent| self.is_connected(*agent))
            .unwrap_or(self.preferred_agent)
    }

    fn input_colors(&self) -> InputColors {
        InputColors {
            text: self.palette.text,
            placeholder: self.palette.muted,
            cursor: self.palette.text,
            selection: self.palette.selection,
        }
    }

    /// Opens the composer on `reference`, or moves an open one to it while
    /// keeping what has been typed.
    fn open_composer(&mut self, reference: LineReference, window: &mut Window, cx: &mut Context<Self>) {
        match &mut self.composer {
            Some(composer) => {
                composer.reference = reference;
                composer.error = None;
            }
            None => {
                let colors = self.input_colors();
                let agent = self.pick_agent();
                let input = cx.new(|cx| TextInput::new("", colors, cx));
                self.composer = Some(Composer {
                    reference,
                    input,
                    agent,
                    sending: false,
                    error: None,
                });
            }
        }
        if let Some(composer) = &self.composer {
            let placeholder = placeholder(composer.agent, &composer.reference);
            composer.input.update(cx, |input, cx| input.set_placeholder(placeholder, cx));
            window.focus(&composer.input.focus_handle(cx));
        }
        cx.notify();
    }

    fn choose_agent(&mut self, agent: Agent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(composer) = &mut self.composer else {
            return;
        };
        composer.agent = agent;
        composer.error = None;
        let placeholder = placeholder(agent, &composer.reference);
        composer.input.update(cx, |input, cx| input.set_placeholder(placeholder, cx));
        window.focus(&composer.input.focus_handle(cx));
        cx.notify();
    }

    fn close_composer(&mut self, _: &CloseComposer, window: &mut Window, cx: &mut Context<Self>) {
        self.composer = None;
        window.focus(&self.focus);
        cx.notify();
    }

    fn send_message(&mut self, _: &SendMessage, _: &mut Window, cx: &mut Context<Self>) {
        let Some(composer) = &mut self.composer else {
            return;
        };
        if composer.sending {
            return;
        }
        let agent = composer.agent;
        let root = self.repo.root().to_path_buf();
        let tie = self.tie.clone().filter(|tie| tie.agent == agent);
        // A session in another folder gets the full path.
        let message = agents::compose_message(
            &composer.reference,
            composer.input.read(cx).text(),
            tie.as_ref().map(|_| root.as_path()),
        );
        let reference = composer.reference.clone();
        composer.sending = true;
        composer.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = runtime::run(async move {
                match tie {
                    Some(tie) => agents::deliver_to(&tie, &message).await,
                    None => agents::deliver(agent, &root, &message).await,
                }
            })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            this.update(cx, |this, cx| {
                match result {
                    Ok(target) => {
                        this.preferred_agent = agent;
                        // Only close the composer if it still holds the line
                        // that was sent; the user may have moved on to another.
                        if this.composer.as_ref().is_some_and(|c| c.reference == reference) {
                            this.composer = None;
                        } else if let Some(composer) = &mut this.composer {
                            composer.sending = false;
                        }
                        this.show_sent_note(format!("Sent to {target}").into(), cx);
                    }
                    Err(error) => {
                        if let Some(composer) = &mut this.composer {
                            composer.sending = false;
                            composer.error = Some(error.into());
                        }
                    }
                }
                this.check_agents(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn show_sent_note(&mut self, note: SharedString, cx: &mut Context<Self>) {
        self.show_note(note, false, cx);
    }

    fn show_error_note(&mut self, note: SharedString, cx: &mut Context<Self>) {
        self.show_note(note, true, cx);
    }

    fn show_note(&mut self, note: SharedString, is_error: bool, cx: &mut Context<Self>) {
        self.sent_note = Some(note);
        self.note_is_error = is_error;
        self.note_generation += 1;
        let generation = self.note_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SENT_NOTE).await;
            this.update(cx, |this, cx| {
                if this.note_generation == generation {
                    this.sent_note = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn render_composer(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let composer = self.composer.as_ref()?;
        let palette = self.palette;
        composer.input.update(cx, |input, _| input.set_colors(self.input_colors()));
        let selected_summary = self.summary(composer.agent);

        let agent_buttons = Agent::ALL.into_iter().map(|agent| {
            let summary = self.summary(agent);
            let tone = if self.tied(agent).is_some() { Tone::Connected } else { summary.tone };
            let is_selected = agent == composer.agent;
            div()
                .id(agent.name())
                .flex()
                .items_center()
                .gap_1p5()
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(if is_selected { palette.muted } else { palette.border })
                .when(is_selected, |el| el.bg(palette.selected))
                .when(!is_selected, |el| el.hover(|el| el.bg(palette.hover)))
                .cursor_pointer()
                .child(div().size(px(7.)).rounded_full().bg(tone_color(tone, palette)))
                .child(agent.name())
                .on_click(cx.listener(move |this, _, window, cx| this.choose_agent(agent, window, cx)))
        });

        let status_line = match &composer.error {
            Some(error) => div().text_color(palette.danger).child(error.clone()),
            None if composer.sending => {
                div().text_color(palette.muted).child(format!("Sending to {}…", composer.agent.name()))
            }
            None if self.tied(composer.agent).is_some() => div().text_color(palette.muted).child(format!(
                "Enter sends to {} in {}, with the file's full path · Esc closes",
                composer.agent.name(),
                self.tied(composer.agent).map(Tie::folder).unwrap_or_default()
            )),
            None if selected_summary.tone != Tone::Connected && !selected_summary.detail.is_empty() => {
                div().text_color(palette.warning).child(selected_summary.detail.clone())
            }
            None => div()
                .text_color(palette.muted)
                .child(format!("Enter sends to {} · Esc closes", composer.agent.name())),
        };

        let preview = if composer.reference.is_whole_file() {
            "the whole file".to_owned()
        } else {
            composer.reference.text.trim().to_owned()
        };
        Some(
            div()
                .key_context(COMPOSER_CONTEXT)
                .on_action(cx.listener(Self::send_message))
                .on_action(cx.listener(Self::close_composer))
                .flex_none()
                .flex()
                .flex_col()
                .gap_2()
                .px_4()
                .py_2p5()
                .border_t_1()
                .border_color(palette.border)
                .bg(palette.sidebar)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_none()
                                .font_family(palette.mono_font)
                                .text_color(palette.text)
                                .child(composer.reference.label()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(palette.mono_font)
                                .text_color(palette.muted)
                                .child(expand_tabs(&preview)),
                        )
                        .child(
                            div()
                                .id("close-composer")
                                .flex_none()
                                .px_1()
                                .cursor_pointer()
                                .text_color(palette.muted)
                                .hover(|el| el.text_color(palette.text))
                                .child("✕")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.composer = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .px_2()
                                .py_1p5()
                                .rounded_md()
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.background)
                                .child(composer.input.clone()),
                        )
                        .children(agent_buttons)
                        .child(
                            div()
                                .id("send")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .font_weight(FontWeight::MEDIUM)
                                .bg(palette.accent)
                                .text_color(palette.on_accent)
                                .when(composer.sending, |el| el.opacity(0.5))
                                .when(!composer.sending, |el| el.cursor_pointer())
                                .child(if composer.sending { "Sending…" } else { "Send" })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.send_message(&SendMessage, window, cx)
                                })),
                        ),
                )
                .child(div().child(status_line)),
        )
    }

    fn open_context_menu(
        &mut self,
        file_id: String,
        at: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_menu = false;
        self.context_menu = Some(ContextMenu { file_id, at });
        window.focus(&self.context_menu_focus);
        cx.notify();
    }

    fn close_context_menu(&mut self, _: &CloseContextMenu, window: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            window.focus(&self.focus);
            cx.notify();
        }
    }

    /// Stages one file — both halves of a rename — then reads the changes
    /// again. The result is reported either way: a stage that fails says so.
    fn stage(&mut self, file_id: String, cx: &mut Context<Self>) {
        let Some(file) = self.file(&file_id).cloned() else {
            return;
        };
        if file.staged || !self.staging.insert(file_id.clone()) {
            return;
        }
        cx.notify();
        let repo = self.repo.clone();
        cx.spawn(async move |this, cx| {
            let path = file.path.clone();
            let result = runtime::run(async move {
                repo.stage_file(&file.path, file.old_path.as_deref()).await
            })
            .await;
            this.update(cx, |this, cx| {
                this.staging.remove(&file_id);
                match result {
                    Ok(Ok(())) => this.show_sent_note(format!("Staged {path}").into(), cx),
                    Ok(Err(error)) => {
                        this.show_error_note(format!("Could not stage {path}: {}", error.message()).into(), cx)
                    }
                    Err(error) => this.show_error_note(format!("Could not stage {path}: {error}").into(), cx),
                }
                // The watcher would notice too; reading now is quicker and
                // costs nothing extra, since reads coalesce.
                this.reload(cx);
            })
            .ok();
        })
        .detach();
    }

    fn stage_active_file(&mut self, _: &StageActiveFile, _: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self.active_file() else {
            return;
        };
        if file.staged {
            let note = format!("{} is already staged", file.path);
            self.show_sent_note(note.into(), cx);
            return;
        }
        let id = file.id.clone();
        self.stage(id, cx);
    }

    /// Opens the file in a tab that stays, and the message box about all of it.
    fn reference_file(&mut self, file_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.close_context_menu(&CloseContextMenu, window, cx);
        let Some(path) = self.target_path(&file_id) else {
            return;
        };
        self.open_file(file_id, false, cx);
        self.open_composer(LineReference::file(path), window, cx);
    }

    /// The right-click menu on a file, where the click was.
    fn render_context_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let menu = self.context_menu.as_ref()?;
        let palette = self.palette;
        let file_id = menu.file_id.clone();
        let can_stage = self.file(&file_id).is_some_and(|f| !f.staged) && !self.staging.contains(&file_id);
        let is_active = self.tabs.active() == Some(file_id.as_str());
        let panel = div()
            .id("context-menu")
            .key_context(CONTEXT_MENU_CONTEXT)
            .track_focus(&self.context_menu_focus)
            .on_action(cx.listener(Self::close_context_menu))
            .occlude()
            .min_w(px(220.))
            .py_1()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(palette.menu_border)
            .bg(palette.sidebar)
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.close_context_menu(&CloseContextMenu, window, cx)
            }))
            .child(
                div()
                    .id("context-menu-reference")
                    .mx_1()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .text_color(palette.text)
                    .hover(|item| item.bg(palette.selected))
                    .child("Reference This File")
                    .on_click(cx.listener({
                        let file_id = file_id.clone();
                        move |this, _, window, cx| this.reference_file(file_id.clone(), window, cx)
                    })),
            )
            .when(can_stage, |panel| {
                panel.child(
                    div()
                        .id("context-menu-stage")
                        .mx_1()
                        .px_2()
                        .py_1()
                        .flex()
                        .gap_6()
                        .rounded_sm()
                        .cursor_pointer()
                        .text_color(palette.text)
                        .hover(|item| item.bg(palette.selected))
                        .child(div().flex_1().child("Stage File"))
                        .when(is_active, |item| item.child(div().text_color(palette.muted).child("⌘↵")))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.close_context_menu(&CloseContextMenu, window, cx);
                            this.stage(file_id.clone(), cx);
                        })),
                )
            });
        Some(
            deferred(
                anchored()
                    .position(menu.at)
                    .snap_to_window_with_margin(px(8.))
                    .child(panel),
            )
            .with_priority(2),
        )
    }

    fn toggle_agent_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.agent_menu = !self.agent_menu;
        self.agent_menu_page = MenuPage::Main;
        if self.agent_menu {
            window.focus(&self.agent_menu_focus);
        } else {
            window.focus(&self.focus);
        }
        cx.notify();
    }

    fn close_agent_menu(&mut self, _: &CloseAgentMenu, window: &mut Window, cx: &mut Context<Self>) {
        self.agent_menu = false;
        window.focus(&self.focus);
        cx.notify();
    }

    /// Switches the menu to the sessions in other folders, and looks for them.
    fn open_session_picker(&mut self, cx: &mut Context<Self>) {
        self.agent_menu_page = MenuPage::Pick(Load::Loading);
        cx.notify();
        let root = self.repo.root().to_path_buf();
        cx.spawn(async move |this, cx| {
            let found = runtime::run(async move { agents::other_sessions(&root).await }).await;
            this.update(cx, |this, cx| {
                if let MenuPage::Pick(load) = &mut this.agent_menu_page {
                    *load = match found {
                        Ok(ties) => Load::Ready(ties),
                        Err(error) => Load::Failed(error.to_string().into()),
                    };
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn tie_to(&mut self, tie: Tie, window: &mut Window, cx: &mut Context<Self>) {
        let note = format!("Messages now go to {} in {}.", tie.agent.name(), tie.folder());
        if let Some(composer) = &mut self.composer {
            composer.agent = tie.agent;
            composer.error = None;
        }
        self.tie = Some(tie);
        self.preferred_agent = self.pick_agent();
        self.close_agent_menu(&CloseAgentMenu, window, cx);
        self.show_sent_note(note.into(), cx);
    }

    /// The menu over the footer's agent status, opening upwards from it like
    /// the one under VS Code's settings cog.
    fn render_agent_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let item = |id: (&'static str, usize)| {
            div()
                .id(id)
                .mx_1()
                .px_2()
                .py_1()
                .flex()
                .items_center()
                .gap_2()
                .rounded_sm()
                .cursor_pointer()
                .text_color(palette.text)
                .hover(|item| item.bg(palette.selected))
        };
        let separator = || div().h_px().mx_2().my_1().bg(palette.border).into_any_element();
        let note = |text: &str, color: Hsla| {
            div().mx_1().px_2().py_1().text_color(color).child(text.to_owned()).into_any_element()
        };

        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        match &self.agent_menu_page {
            MenuPage::Main => {
                if let Some(tie) = &self.tie {
                    rows.push(
                        item(("agent-menu-untie", 0))
                            .child(div().flex_1().child(format!("Disconnect From {}", tie.folder())))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.tie = None;
                                this.close_agent_menu(&CloseAgentMenu, window, cx);
                            }))
                            .into_any_element(),
                    );
                }
                rows.push(
                    item(("agent-menu-pick", 0))
                        .child(div().flex_1().child("Pick Session From Another Folder…"))
                        .on_click(cx.listener(|this, _, _, cx| this.open_session_picker(cx)))
                        .into_any_element(),
                );
                rows.push(separator());
                // Placeholders: these only close the menu for now.
                let placeholders: [Option<(&str, Option<&str>)>; 6] = [
                    Some(("Start Claude Code Here", None)),
                    Some(("Start Codex Here", None)),
                    None,
                    Some(("Copy Start Command", Some("⇧⌘C"))),
                    Some(("Check Again", Some("⌘R"))),
                    Some(("Agent Settings…", Some("⌘,"))),
                ];
                for (ix, entry) in placeholders.into_iter().enumerate() {
                    rows.push(match entry {
                        None => separator(),
                        Some((label, keys)) => item(("agent-menu-item", ix))
                            .gap_6()
                            .child(div().flex_1().child(label.to_owned()))
                            .when_some(keys, |item, keys| {
                                item.child(div().text_color(palette.muted).child(keys.to_owned()))
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_agent_menu(&CloseAgentMenu, window, cx);
                                if label == "Agent Settings…" {
                                    crate::settings_view::open(cx);
                                }
                            }))
                            .into_any_element(),
                    });
                }
            }
            MenuPage::Pick(load) => {
                rows.push(
                    item(("agent-menu-back", 0))
                        .text_color(palette.muted)
                        .child("‹")
                        .child(div().flex_1().child("Sessions in Other Folders"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.agent_menu_page = MenuPage::Main;
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                rows.push(separator());
                match load {
                    Load::Loading => rows.push(note("Looking…", palette.muted)),
                    Load::Failed(error) => rows.push(note(error, palette.danger)),
                    Load::Ready(ties) if ties.is_empty() => rows.push(note(
                        "No session in another folder can take messages. Start Claude Code with the onlydiffs channel, or Codex, in that folder.",
                        palette.muted,
                    )),
                    Load::Ready(ties) => {
                        for (ix, tie) in ties.iter().enumerate() {
                            let chosen = tie.clone();
                            let is_current = self.tie.as_ref() == Some(tie);
                            rows.push(
                                item(("agent-menu-session", ix))
                                    .when(is_current, |row| row.bg(palette.selected))
                                    .child(agent_logo(tie.agent, palette))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_col()
                                            .child(div().truncate().child(tie.folder()))
                                            .child(
                                                div().truncate().text_color(palette.muted).child(
                                                    tie.title.clone().unwrap_or_else(|| {
                                                        format!("{} · pid {}", tie.agent.name(), tie.pid)
                                                    }),
                                                ),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.tie_to(chosen.clone(), window, cx)
                                    }))
                                    .into_any_element(),
                            );
                        }
                    }
                }
            }
        }

        let menu = div()
            .id("agent-menu")
            .key_context(AGENT_MENU_CONTEXT)
            .track_focus(&self.agent_menu_focus)
            .on_action(cx.listener(Self::close_agent_menu))
            .occlude()
            .min_w(px(260.))
            .max_w(px(420.))
            .py_1()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(palette.menu_border)
            .bg(palette.sidebar)
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                if !this.over_agent_status {
                    this.close_agent_menu(&CloseAgentMenu, window, cx);
                }
            }))
            .children(rows);
        deferred(
            anchored()
                .anchor(Corner::BottomLeft)
                .offset(point(px(-6.), px(-6.)))
                .snap_to_window_with_margin(px(8.))
                .child(menu),
        )
        .with_priority(1)
    }

    /// Makes a footer status open the agent menu when clicked.
    fn menu_trigger(&self, status: gpui::Stateful<gpui::Div>, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        div()
            .relative()
            .child(
                status
                    .px_1p5()
                    .mx_neg_1p5()
                    .rounded_sm()
                    .cursor_pointer()
                    .when(self.agent_menu, |item| item.bg(palette.selected))
                    .hover(|item| item.bg(palette.hover))
                    .on_hover(cx.listener(|this, hovered: &bool, _, _| {
                        this.over_agent_status = *hovered;
                    }))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_agent_menu(window, cx))),
            )
            .when(self.agent_menu, |item| {
                item.child(div().absolute().top_0().left_0().child(self.render_agent_menu(cx)))
            })
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let overall = summary::overall(&[
            (Agent::Claude, self.summary(Agent::Claude)),
            (Agent::Codex, self.summary(Agent::Codex)),
        ]);
        // The tooltip would sit on top of the open menu.
        let tip = |detail: String| if self.agent_menu { String::new() } else { detail };
        let footer = div()
            .h(px(FOOTER_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap_4()
            .px_3()
            .border_t_1()
            .border_color(palette.border)
            .bg(palette.sidebar);
        let footer = match (&self.tie, overall) {
            // Tied: that session, and any in the repository that connect too.
            (Some(tie), overall) => {
                let detail = format!(
                    "Messages go to {} in {}, with each file's full path. Click to change.",
                    tie.title.as_deref().map(|t| format!("“{t}”")).unwrap_or_else(|| tie.agent.name().to_owned()),
                    tie.cwd.display()
                );
                let tied = connected_item(
                    tie.agent,
                    format!("{} Connected · {}", tie.agent.name(), tie.folder()),
                    tip(detail),
                    palette,
                );
                let here = match overall {
                    summary::Overall::Connected(agents) => agents,
                    _ => Vec::new(),
                };
                footer.child(self.menu_trigger(tied, cx)).children(
                    here.into_iter()
                        .filter(|(agent, _)| *agent != tie.agent)
                        .map(|(agent, detail)| {
                            connected_item(agent, format!("{} Connected", agent.name()), detail, palette)
                        }),
                )
            }
            (None, summary::Overall::Checking) => footer,
            (None, summary::Overall::Connected(agents)) => footer.children(agents.into_iter().map(
                |(agent, detail)| connected_item(agent, format!("{} Connected", agent.name()), detail, palette),
            )),
            (None, summary::Overall::Unreachable(detail)) => footer.child(self.menu_trigger(
                status_item("agents-unreachable", "Unreachable Agent", palette.warning, tip(detail), palette),
                cx,
            )),
            (None, summary::Overall::NoneConnected(detail)) => footer.child(self.menu_trigger(
                status_item("agents-none", "No Connected Agents", palette.muted, tip(detail), palette),
                cx,
            )),
        };
        footer
            .child(div().flex_1())
            .when_some(self.sent_note.clone(), |footer, note| {
                let color = if self.note_is_error { palette.danger } else { palette.added_text };
                footer.child(div().text_color(color).child(note))
            })
            .children(update_item(cx, palette))
    }

    /// Reads the list of changed files again, then every open tab.
    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.reloading {
            self.reload_again = true;
            return;
        }
        self.reloading = true;
        let first = matches!(self.diff, Load::Loading);
        let repo = self.repo.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime::run(async move { tokio::join!(repo.diff(), repo.list_files()) }).await;
            let (loaded, listed) = match result {
                Ok((Ok(diff), listed)) => (Load::Ready(diff), listed.unwrap_or_default()),
                Ok((Err(error), _)) => (Load::Failed(error.message().to_owned().into()), Vec::new()),
                Err(error) => (Load::Failed(error.to_string().into()), Vec::new()),
            };
            this.update(cx, |this, cx| {
                this.reloading = false;
                let before: HashMap<String, String> =
                    this.files().iter().map(|f| (f.id.clone(), f.path.clone())).collect();
                this.diff = loaded;
                this.set_tree(listed, first);
                let ids: Vec<String> = this.files().iter().map(|f| f.id.clone()).collect();
                // A tab follows its file when the file's entry changes under
                // it: staging turns `unstaged:a.rs` into `staged:a.rs`; a
                // commit leaves it unchanged, shown whole; a fresh edit turns
                // an unchanged file into a diff.
                for key in this.tabs.keys().to_vec() {
                    if let Some(path) = key.strip_prefix(PLAIN) {
                        if let Some(change) = this.change_for_path(path) {
                            let id = change.id.clone();
                            this.tabs.replace(&key, &id);
                        }
                        continue;
                    }
                    if ids.contains(&key) {
                        continue;
                    }
                    let Some(path) = before.get(&key).cloned() else { continue };
                    let moved = match this.change_for_path(&path) {
                        Some(change) => Some(change.id.clone()),
                        None => this.all_paths.contains(&path).then(|| format!("{PLAIN}{path}")),
                    };
                    if let Some(new) = moved {
                        this.tabs.replace(&key, &new);
                    }
                }
                // A file that is gone altogether has nothing left to show.
                let this_ref = &*this;
                let keep: HashSet<String> = this_ref
                    .tabs
                    .keys()
                    .iter()
                    .filter(|key| this_ref.target_path(key).is_some())
                    .cloned()
                    .collect();
                this.tabs.retain(|key| keep.contains(key));
                for key in this.tabs.keys().to_vec() {
                    this.load(key, cx);
                }
                // Open something on the first read, so the pane is not empty.
                // Later reads leave it alone: the user may have closed every
                // tab on purpose.
                if first
                    && this.tabs.len() == 0
                    && let Some(first) = ids.first()
                {
                    this.open_file(first.clone(), true, cx);
                }
                this.tabs_changed(cx);
                this.start_triage(cx);
                if std::mem::take(&mut this.reload_again) {
                    this.reload(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Checks every changed file against the triage rules, in the
    /// background: reading each file's two versions is the slow part.
    fn start_triage(&mut self, cx: &mut Context<Self>) {
        self.triage_generation += 1;
        let generation = self.triage_generation;
        let files: Vec<FileChange> = self.files().to_vec();
        let ids: HashSet<&str> = files.iter().map(|f| f.id.as_str()).collect();
        self.triage.retain(|id, _| ids.contains(id.as_str()));
        let repo = self.repo.clone();
        cx.spawn(async move |this, cx| {
            for file in files {
                if file.binary || file.error.is_some() {
                    continue;
                }
                let repo = repo.clone();
                let read = file.clone();
                let verdict = runtime::run(async move {
                    let contents = repo
                        .file_contents(&read.path, read.old_path.as_deref(), read.status, read.staged)
                        .await
                        .ok()?;
                    crate::triage::judge(
                        &read.path,
                        &contents.old_contents.unwrap_or_default(),
                        &contents.new_contents.unwrap_or_default(),
                    )
                })
                .await
                .ok()
                .flatten();
                let current = this.update(cx, |this, cx| {
                    if this.triage_generation != generation {
                        return false;
                    }
                    match verdict {
                        Some(verdict) => {
                            this.triage.insert(file.id.clone(), verdict);
                        }
                        None => {
                            this.triage.remove(&file.id);
                        }
                    }
                    cx.notify();
                    true
                });
                if !current.unwrap_or(false) {
                    return;
                }
            }
        })
        .detach();
    }

    fn files(&self) -> &[FileChange] {
        match &self.diff {
            Load::Ready(diff) => &diff.files,
            _ => &[],
        }
    }

    /// Takes a fresh file listing: rebuilds the tree, and on the first read
    /// opens the folders that hold changes, so they are in view.
    fn set_tree(&mut self, listed: Vec<String>, first: bool) {
        let mut paths: HashSet<String> = listed.into_iter().collect();
        // A deletion is not in the listing, but belongs in the tree.
        paths.extend(self.files().iter().map(|f| f.path.clone()));
        self.tree = crate::explorer::build(paths.iter().map(String::as_str));
        self.all_paths = paths;
        if first {
            let changed: Vec<String> = self.files().iter().map(|f| f.path.clone()).collect();
            for path in &changed {
                self.expanded.extend(crate::explorer::ancestors(path));
            }
        }
        self.rebuild_rows();
    }

    /// Opens the folders down to `path`, highlights it, and scrolls to it.
    fn reveal(&mut self, path: &str) {
        let before = self.expanded.len();
        self.expanded.extend(crate::explorer::ancestors(path));
        if self.expanded.len() != before {
            self.rebuild_rows();
        }
        self.explorer_selection = Some(path.to_owned());
        if let Some(ix) = self.rows.iter().position(|row| row.path == path) {
            self.explorer_scroll.scroll_to_item(ix, gpui::ScrollStrategy::Center);
        }
    }

    fn toggle_dir(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_owned());
        }
        self.explorer_selection = Some(path.to_owned());
        self.rebuild_rows();
        cx.notify();
    }

    fn selected_row(&self) -> Option<(usize, crate::explorer::Row)> {
        let selected = self.explorer_selection.as_ref()?;
        self.rows.iter().position(|row| &row.path == selected).map(|ix| (ix, self.rows[ix].clone()))
    }

    fn select_row(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get(ix) {
            self.explorer_selection = Some(row.path.clone());
            self.explorer_scroll.scroll_to_item(ix, gpui::ScrollStrategy::Top);
            cx.notify();
        }
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let next = self.selected_row().map_or(0, |(ix, _)| (ix + 1).min(self.rows.len().saturating_sub(1)));
        self.select_row(next, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        let previous = self.selected_row().map_or(0, |(ix, _)| ix.saturating_sub(1));
        self.select_row(previous, cx);
    }

    /// Zed's left arrow: close an open folder, else go to the one above.
    fn collapse_selected(&mut self, _: &CollapseSelected, _: &mut Window, cx: &mut Context<Self>) {
        let Some((ix, row)) = self.selected_row() else { return };
        if row.kind == (crate::explorer::RowKind::Dir { expanded: true }) {
            self.toggle_dir(&row.path, cx);
            return;
        }
        let parent = self.rows[..ix].iter().rposition(|r| r.depth < row.depth);
        if let Some(parent) = parent {
            self.select_row(parent, cx);
        }
    }

    /// Zed's right arrow: open a closed folder, else step into it.
    fn expand_selected(&mut self, _: &ExpandSelected, _: &mut Window, cx: &mut Context<Self>) {
        let Some((ix, row)) = self.selected_row() else { return };
        match row.kind {
            crate::explorer::RowKind::Dir { expanded: false } => self.toggle_dir(&row.path, cx),
            crate::explorer::RowKind::Dir { expanded: true } => self.select_row(ix + 1, cx),
            crate::explorer::RowKind::File => {}
        }
    }

    fn collapse_all(&mut self, _: &CollapseAll, _: &mut Window, cx: &mut Context<Self>) {
        self.expanded.clear();
        self.rebuild_rows();
        cx.notify();
    }

    fn expand_all(&mut self, _: &ExpandAll, _: &mut Window, cx: &mut Context<Self>) {
        crate::explorer::all_dirs(&self.tree, &mut self.expanded);
        self.rebuild_rows();
        cx.notify();
    }

    /// Space or return: open the highlighted file in a tab that stays, or
    /// open or close the highlighted folder.
    fn open_selected(&mut self, _: &OpenSelected, _: &mut Window, cx: &mut Context<Self>) {
        let Some((_, row)) = self.selected_row() else { return };
        match row.kind {
            crate::explorer::RowKind::File => {
                let key = self.key_for_path(&row.path);
                self.open_file(key, false, cx);
            }
            _ => self.toggle_dir(&row.path, cx),
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows = crate::explorer::rows(&self.tree, &self.expanded);
    }

    /// The change for `path` a click opens: the working-tree one when there
    /// are both, since that is what is on disk.
    fn change_for_path(&self, path: &str) -> Option<&FileChange> {
        let mut found = self.files().iter().filter(|f| f.path == path);
        let first = found.next()?;
        if first.staged { found.next().or(Some(first)) } else { Some(first) }
    }

    /// The tab key for a file: its change, or the file shown whole.
    fn key_for_path(&self, path: &str) -> String {
        match self.change_for_path(path) {
            Some(change) => change.id.clone(),
            None => format!("{PLAIN}{path}"),
        }
    }

    /// The path a tab shows, if the file is still there.
    fn target_path(&self, key: &str) -> Option<String> {
        match key.strip_prefix(PLAIN) {
            Some(path) => self.all_paths.contains(path).then(|| path.to_owned()),
            None => self.file(key).map(|f| f.path.clone()),
        }
    }

    fn active_path(&self) -> Option<String> {
        self.target_path(self.tabs.active()?)
    }

    fn file(&self, id: &str) -> Option<&FileChange> {
        self.files().iter().find(|file| file.id == id)
    }

    /// The file in the active tab.
    fn active_file(&self) -> Option<&FileChange> {
        self.file(self.tabs.active()?)
    }

    /// Opens a file in a tab — the preview tab when `preview` — and shows it.
    fn open_file(&mut self, id: String, preview: bool, cx: &mut Context<Self>) {
        if self.target_path(&id).is_none() {
            return;
        }
        if self.tabs.open(&id, preview) {
            self.load(id, cx);
        }
        self.tabs_changed(cx);
    }

    /// Settles everything that hangs off the tabs after they change.
    fn tabs_changed(&mut self, cx: &mut Context<Self>) {
        let tabs = &self.tabs;
        self.opened.retain(|key, _| tabs.position(key).is_some());
        // A message is about a line in the file on screen; moving to another
        // file lets it go rather than carrying a stale reference along.
        let path = self.active_path();
        if self.composer.as_ref().is_some_and(|c| Some(&c.reference.path) != path.as_ref()) {
            self.composer = None;
        }
        if let Some(ix) = self.tabs.active_index() {
            self.tab_scroll.scroll_to_item(ix);
        }
        // Zed's auto-reveal: switching to a tab opens the folders down to
        // its file and highlights it.
        let active = self.tabs.active().map(str::to_owned);
        if active != self.revealed {
            self.revealed = active;
            if let Some(path) = path.clone() {
                self.reveal(&path);
            }
        }
        cx.notify();
    }

    /// Reads the file in tab `id` and works out its diff.
    fn load(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(path) = id.strip_prefix(PLAIN).map(str::to_owned) {
            self.load_plain(id, path, cx);
            return;
        }
        let Some(file) = self.file(&id).cloned() else {
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        let opened = self.opened.entry(id.clone()).or_insert_with(|| Opened {
            diff: Load::Loading,
            syntax: Syntax::default(),
            scroll: UniformListScrollHandle::new(),
            generation,
        });
        opened.generation = generation;

        if file.binary {
            opened.diff = Load::Failed("Binary file — no text diff.".into());
            return;
        }
        if let Some(error) = &file.error {
            opened.diff = Load::Failed(error.clone().into());
            return;
        }

        let repo = self.repo.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime::run(async move {
                let contents = repo
                    .file_contents(&file.path, file.old_path.as_deref(), file.status, file.staged)
                    .await
                    .map_err(|error| error.message().to_owned())?;
                let old = contents.old_contents.unwrap_or_default();
                let new = contents.new_contents.unwrap_or_default();
                let diff = engine::diff_texts(&old, &new, engine::CONTEXT_LINES);
                // Whole files, so a line inside a block comment still reads
                // as a comment. A rename is highlighted by each side's name.
                let syntax = Syntax {
                    old: crate::syntax::highlight(file.old_path.as_deref().unwrap_or(&file.path), &old),
                    new: crate::syntax::highlight(&file.path, &new),
                };
                Ok::<_, String>((diff, syntax))
            })
            .await;
            let (loaded, syntax) = match result {
                Ok(Ok((diff, syntax))) => (Load::Ready(Arc::new(diff)), syntax),
                Ok(Err(message)) => (Load::Failed(message.into()), Syntax::default()),
                Err(error) => (Load::Failed(error.to_string().into()), Syntax::default()),
            };
            this.update(cx, |this, cx| {
                if let Some(opened) = this.opened.get_mut(&id)
                    && opened.generation == generation
                {
                    opened.diff = loaded;
                    opened.syntax = syntax;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Reads an unchanged file to show it whole.
    fn load_plain(&mut self, id: String, path: String, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let opened = self.opened.entry(id.clone()).or_insert_with(|| Opened {
            diff: Load::Loading,
            syntax: Syntax::default(),
            scroll: UniformListScrollHandle::new(),
            generation,
        });
        opened.generation = generation;
        let repo = self.repo.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime::run(async move {
                let bytes = repo
                    .read_file(std::path::Path::new(&path), PLAIN_LIMIT)
                    .await
                    .map_err(|error| error.message().to_owned())?;
                if bytes.contains(&0) {
                    return Err("Binary file.".to_owned());
                }
                let text = String::from_utf8_lossy(&bytes);
                let syntax = Syntax { old: None, new: crate::syntax::highlight(&path, &text) };
                Ok((engine::plain(&text), syntax))
            })
            .await;
            let (loaded, syntax) = match result {
                Ok(Ok((diff, syntax))) => (Load::Ready(Arc::new(diff)), syntax),
                Ok(Err(message)) => (Load::Failed(message.into()), Syntax::default()),
                Err(error) => (Load::Failed(error.to_string().into()), Syntax::default()),
            };
            this.update(cx, |this, cx| {
                if let Some(opened) = this.opened.get_mut(&id)
                    && opened.generation == generation
                {
                    opened.diff = loaded;
                    opened.syntax = syntax;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn close_tab(&mut self, _: &CloseTab, _: &mut Window, cx: &mut Context<Self>) {
        self.tabs.close_active();
        self.tabs_changed(cx);
    }

    fn close_other_tabs(&mut self, _: &CloseOtherTabs, _: &mut Window, cx: &mut Context<Self>) {
        self.tabs.close_others();
        self.tabs_changed(cx);
    }

    fn next_tab(&mut self, _: &NextTab, _: &mut Window, cx: &mut Context<Self>) {
        self.tabs.cycle(true);
        self.tabs_changed(cx);
    }

    fn previous_tab(&mut self, _: &PreviousTab, _: &mut Window, cx: &mut Context<Self>) {
        self.tabs.cycle(false);
        self.tabs_changed(cx);
    }

    fn activate_tab(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.tabs.activate(ix);
        self.tabs_changed(cx);
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let active = self.tabs.active_index();
        let infos: Vec<Option<(String, String, Option<&FileChange>)>> = self
            .tabs
            .keys()
            .iter()
            .map(|key| self.target_path(key).map(|path| (key.clone(), path, self.file(key))))
            .collect();
        let name = |path: &str| path.rsplit('/').next().unwrap_or(path).to_owned();

        let tabs = infos.iter().enumerate().filter_map(|(ix, info)| {
            let (key, path, change) = info.as_ref()?;
            let (key, change) = (key.clone(), *change);
            let is_active = active == Some(ix);
            let is_preview = self.tabs.is_preview(&key);
            let label = name(path);
            // Zed's rule: when two tabs share a name, say where each one is.
            let clashes = infos.iter().flatten().filter(|(_, other, _)| name(other) == label).count() > 1;
            let staged = change.is_some_and(|c| c.staged);
            let detail = clashes
                .then(|| match path.rsplit_once('/') {
                    Some((dir, _)) => dir.to_owned(),
                    None => "/".to_owned(),
                })
                .map(|dir| if staged { format!("{dir} · staged") } else { dir });

            let close_key = key.clone();
            Some(
                div()
                    .id(SharedString::from(format!("tab-{key}")))
                    .group("tab")
                    .flex_none()
                    .max_w(px(TAB_MAX_WIDTH))
                    .h_full()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .pl_3()
                    .pr_1p5()
                    .border_r_1()
                    .border_color(palette.border)
                    .cursor_pointer()
                    .map(|tab| {
                        if is_active {
                            tab.bg(palette.background).text_color(palette.text)
                        } else {
                            tab.bg(palette.sidebar)
                                .text_color(palette.muted)
                                .border_b_1()
                                .hover(|tab| tab.text_color(palette.text))
                        }
                    })
                    .on_mouse_down(gpui::MouseButton::Right, {
                        let key = key.clone();
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.open_context_menu(key.clone(), event.position, window, cx)
                        })
                    })
                    .on_mouse_down(gpui::MouseButton::Left, {
                        let key = key.clone();
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            if event.modifiers.control {
                                this.open_context_menu(key.clone(), event.position, window, cx)
                            }
                        })
                    })
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                        if event.modifiers().control {
                            return;
                        }
                        if event.click_count() > 1 {
                            this.tabs.keep(&key);
                        }
                        if let Some(ix) = this.tabs.position(&key) {
                            this.activate_tab(ix, cx);
                        }
                    }))
                    .when_some(change, |tab, change| {
                        tab.child(
                            div()
                                .flex_none()
                                .font_family(palette.mono_font)
                                .text_color(status_color(change.status, palette))
                                .child(status_letter(change.status)),
                        )
                    })
                    .child(file_icon(path, palette))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .when(is_preview, |label| label.italic())
                            .child(label),
                    )
                    .when_some(detail, |tab, detail| {
                        tab.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(palette.muted)
                                .when(is_preview, |label| label.italic())
                                .child(detail),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("close-{close_key}")))
                            .flex_none()
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .text_color(palette.muted)
                            .when(!is_active, |close| {
                                close.invisible().group_hover("tab", |close| close.visible())
                            })
                            .hover(|close| close.bg(palette.hover).text_color(palette.text))
                            .child("✕")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if let Some(ix) = this.tabs.position(&close_key) {
                                    this.tabs.close(ix);
                                    this.tabs_changed(cx);
                                }
                            })),
                    ),
            )
        });

        div()
            .id("tab-bar")
            .flex_none()
            .h(px(TAB_HEIGHT))
            .flex()
            .bg(palette.sidebar)
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll)
            .children(tabs)
            // The rest of the bar, carrying the line the active tab breaks.
            .child(
                div()
                    .flex_1()
                    .min_w(px(24.))
                    .h_full()
                    .border_b_1()
                    .border_color(palette.border),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let header = match &self.diff {
            Load::Ready(diff) => {
                let name = diff
                    .repo_path
                    .rsplit('/')
                    .find(|part| !part.is_empty())
                    .unwrap_or(&diff.repo_path)
                    .to_owned();
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                    .child(
                        div()
                            .text_color(palette.muted)
                            .child(format!("{} · {} changed", diff.branch, diff.files.len())),
                    )
            }
            Load::Loading => div().text_color(palette.muted).child("Reading repository…"),
            Load::Failed(_) => div().text_color(palette.danger).child("Could not read repository"),
        };

        let count = self.rows.len();
        let list = uniform_list(
            "tree",
            count,
            cx.processor(|this, range: std::ops::Range<usize>, window, cx| {
                let palette = this.palette;
                let active_path = this.active_path();
                let selection = this.explorer_selection.clone();
                let focused = this.explorer_focus.is_focused(window);
                let context_target = this.context_menu.as_ref().map(|menu| menu.file_id.clone());
                let changed_dirs: HashSet<String> = this
                    .files()
                    .iter()
                    .flat_map(|f| crate::explorer::ancestors(&f.path).collect::<Vec<_>>())
                    .collect();
                let start = range.start;
                this.rows[range]
                    .to_vec()
                    .into_iter()
                    .enumerate()
                    .map(|(offset, row)| {
                        let change = match row.kind {
                            crate::explorer::RowKind::File => this.change_for_path(&row.path).cloned(),
                            _ => None,
                        };
                        let staged_too = change.is_some()
                            && this.files().iter().filter(|f| f.path == row.path).count() > 1;
                        let is_open = active_path.as_deref() == Some(row.path.as_str());
                        let is_selected = focused && selection.as_deref() == Some(row.path.as_str());
                        let key = this.key_for_path(&row.path);
                        let in_menu = context_target.as_deref() == Some(key.as_str());
                        this.render_tree_row(
                            start + offset,
                            row,
                            change,
                            staged_too,
                            is_open,
                            is_selected,
                            in_menu,
                            changed_dirs.contains(&this.rows[start + offset].path),
                            key,
                            palette,
                            cx,
                        )
                    })
                    .collect()
            }),
        )
        .track_scroll(self.explorer_scroll.clone())
        .flex_1();

        let list = div()
            .key_context(TREE_CONTEXT)
            .track_focus(&self.explorer_focus)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::collapse_selected))
            .on_action(cx.listener(Self::expand_selected))
            .on_action(cx.listener(Self::collapse_all))
            .on_action(cx.listener(Self::expand_all))
            .on_action(cx.listener(Self::open_selected))
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .py_1()
            .child(list);

        div()
            .w(px(SIDEBAR_WIDTH))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.sidebar)
            .border_r_1()
            .border_color(palette.border)
            .child(
                div()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(palette.border)
                    .child(header),
            )
            .child(list)
    }

    /// One row of the project tree.
    #[allow(clippy::too_many_arguments)]
    fn render_tree_row(
        &self,
        ix: usize,
        row: crate::explorer::Row,
        change: Option<FileChange>,
        staged_too: bool,
        is_open: bool,
        is_selected: bool,
        in_menu: bool,
        holds_changes: bool,
        key: String,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        use crate::explorer::RowKind;
        let indent = 8. + row.depth as f32 * INDENT;
        let element = div()
            .id(("tree-row", ix))
            .group("file-row")
            .relative()
            .w_full()
            .h(px(ROW_HEIGHT + 4.))
            .flex()
            .items_center()
            .gap_1p5()
            .pl(px(indent))
            .pr_2()
            .border_1()
            .border_color(if is_selected { palette.accent } else { gpui::transparent_black() })
            .cursor_pointer()
            .when(is_open, |r| r.bg(palette.selected))
            .when(!is_open && in_menu, |r| r.bg(palette.hover))
            .when(!is_open, |r| r.hover(|r| r.bg(palette.hover)))
            // Indent guides: a line down the middle of each level's chevron.
            .children((0..row.depth).map(|level| {
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(8. + level as f32 * INDENT + 6.))
                    .w_px()
                    .bg(palette.guide)
            }));

        match row.kind {
            RowKind::Dir { expanded } => {
                let path = row.path.clone();
                element
                    .child(
                        div()
                            .w(px(12.))
                            .flex_none()
                            .text_color(palette.muted)
                            .child(if expanded { "▾" } else { "▸" }),
                    )
                    .child(
                        gpui::img(crate::file_icons::folder_icon(expanded, palette.icons))
                            .size(px(16.))
                            .flex_none(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(if holds_changes { palette.modified } else { palette.text })
                            .child(row.label.clone()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.explorer_focus);
                        this.toggle_dir(&path, cx);
                    }))
                    .into_any_element()
            }
            RowKind::File => {
                let path = row.path.clone();
                let open_key = key.clone();
                let menu_key = key.clone();
                let ctrl_key = key.clone();
                let name_color = match &change {
                    Some(c) => status_color(c.status, palette),
                    None => palette.text,
                };
                let triage = change.as_ref().and_then(|c| self.triage.get(&c.id).cloned());
                let pending = change.as_ref().is_some_and(|c| self.staging.contains(&c.id));
                element
                    .child(div().w(px(12.)).flex_none())
                    .child(file_icon(&row.path, palette))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(name_color)
                            .when(change.as_ref().is_some_and(|c| c.status == ChangeStatus::Deleted), |name| {
                                name.line_through()
                            })
                            .child(row.label.clone()),
                    )
                    .when_some(change.as_ref().and_then(|c| triage.map(|t| (c.id.clone(), t))), |r, (id, t)| {
                        r.child(triage_marker(&id, &t, palette))
                    })
                    .when(staged_too || change.as_ref().is_some_and(|c| c.staged), |r| {
                        r.child(div().flex_none().text_color(palette.muted).child("staged"))
                    })
                    .when_some(change.clone(), |r, c| {
                        r.child(
                            div()
                                .flex_none()
                                .flex()
                                .gap_1()
                                .font_family(palette.mono_font)
                                .when(c.additions > 0, |el| {
                                    el.child(div().text_color(palette.added_text).child(format!("+{}", c.additions)))
                                })
                                .when(c.deletions > 0, |el| {
                                    el.child(div().text_color(palette.removed_text).child(format!("−{}", c.deletions)))
                                }),
                        )
                    })
                    .when_some(change.clone().filter(|c| !c.staged), |r, c| {
                        let id = c.id.clone();
                        r.child(
                            div()
                                .id(SharedString::from(format!("stage-{}", c.id)))
                                .flex_none()
                                .size(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_sm()
                                .font_family(palette.mono_font)
                                .text_color(palette.muted)
                                .when(!pending, |button| {
                                    button
                                        .invisible()
                                        .group_hover("file-row", |button| button.visible())
                                        .cursor_pointer()
                                        .hover(|button| button.bg(palette.selected).text_color(palette.text))
                                        .with_tip("Stage File".into(), palette)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.stage(id.clone(), cx);
                                        }))
                                })
                                .child(if pending { "…" } else { "+" }),
                        )
                    })
                    .when_some(change.clone(), |r, c| {
                        r.child(
                            div()
                                .flex_none()
                                .w(px(12.))
                                .font_family(palette.mono_font)
                                .text_color(status_color(c.status, palette))
                                .child(status_letter(c.status)),
                        )
                    })
                    .on_click(cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                        // A control-click is the Mac's other right click.
                        if event.modifiers().control {
                            return;
                        }
                        window.focus(&this.explorer_focus);
                        this.explorer_selection = Some(path.clone());
                        // Zed's project panel: one click previews, two keep.
                        this.open_file(open_key.clone(), event.click_count() < 2, cx);
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            this.open_context_menu(menu_key.clone(), event.position, window, cx)
                        }),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            if event.modifiers.control {
                                this.open_context_menu(ctrl_key.clone(), event.position, window, cx)
                            }
                        }),
                    )
                    .into_any_element()
            }
        }
    }

    /// One side-by-side row: the old version's line on the left, the new
    /// version's on the right, each clickable to message an agent about it.
    #[allow(clippy::too_many_arguments)]
    fn render_split_row(
        &self,
        ix: usize,
        row: &crate::engine::SplitRow,
        syntax: &Syntax,
        path: &str,
        referenced: Option<&LineReference>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (left, right) = match row {
            crate::engine::SplitRow::Fold { hidden } => {
                return render_row(&Row::Fold { hidden: *hidden }, None, palette, false);
            }
            crate::engine::SplitRow::Pair { left, right } => (left, right),
        };
        let cell = |side: &'static str, line: &Option<Line>, cx: &mut Context<Self>| {
            let Some(line) = line else {
                // Nothing on this side — the other version has a line here —
                // drawn as Pierre does, in diagonal stripes.
                return div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .bg(gpui::pattern_slash(Hsla { a: 0.5, ..palette.menu_border }, 1., 6.))
                    .into_any_element();
            };
            let background = match line.kind {
                RowKind::Context => None,
                RowKind::Added => Some(palette.added_bg),
                RowKind::Removed => Some(palette.removed_bg),
            };
            let number = if side == "left" { line.old_number } else { line.new_number };
            let reference = line_reference(path, line);
            let is_referenced = referenced == Some(&reference);
            div()
                .id((side, ix))
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .overflow_hidden()
                .cursor_pointer()
                .when_some(background, |el, bg| el.bg(bg))
                .when(is_referenced, |el| el.bg(palette.selected))
                .when(!is_referenced, |el| el.hover(|el| el.bg(palette.hover)))
                .child(
                    div()
                        .w(px(NUMBER_WIDTH))
                        .flex_none()
                        .pr_2()
                        .flex()
                        .justify_end()
                        .text_color(palette.muted)
                        .child(number.map(|n| n.to_string()).unwrap_or_default()),
                )
                .child(change_bar(line.kind, palette))
                .child(div().flex_1().min_w_0().child(highlighted(line, syntax.spans(line), palette)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(key) = this.tabs.active().map(str::to_owned) {
                        this.tabs.keep(&key);
                    }
                    this.open_composer(reference.clone(), window, cx)
                }))
                .into_any_element()
        };
        div()
            .h(px(ROW_HEIGHT))
            .flex()
            .font_family(palette.mono_font)
            .whitespace_nowrap()
            .child(cell("left", left, cx))
            .child(div().w_px().h_full().flex_none().bg(palette.border))
            .child(cell("right", right, cx))
            .into_any_element()
    }

    fn render_main(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = self.palette;
        let pane = div().flex_1().min_w_0().h_full().flex().flex_col();

        if let Load::Failed(message) = &self.diff {
            return pane.child(message_block(message.clone(), palette.danger));
        }
        let (Some(key), Some(path)) = (self.tabs.active().map(str::to_owned), self.active_path()) else {
            return pane.child(self.render_tab_bar(cx)).child(message_block(
                "Select a file in the tree.".into(),
                palette.muted,
            ));
        };
        let change = self.file(&key);
        let opened = self.opened.get(&key);

        let header = div()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(palette.border)
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .font_family(palette.mono_font)
                    .text_color(palette.text)
                    .child(match change.and_then(|c| c.old_path.as_ref()) {
                        Some(old) if old != &path => format!("{old} → {path}"),
                        _ => path.clone(),
                    }),
            )
            .child(div().text_color(palette.muted).child(match change {
                Some(c) if c.staged => "staged",
                Some(_) => "unstaged",
                None => "unchanged",
            }));

        let body = match opened.map(|o| &o.diff) {
            None | Some(Load::Loading) => {
                message_block("Loading…".into(), palette.muted).into_any_element()
            }
            Some(Load::Failed(message)) => {
                message_block(message.clone(), palette.danger).into_any_element()
            }
            Some(Load::Ready(diff)) if diff.rows.is_empty() => {
                let note = if change.is_none() { "Empty file." } else { "No line changes (mode or whitespace only)." };
                message_block(note.into(), palette.muted)
                    .into_any_element()
            }
            Some(Load::Ready(diff)) => {
                let side_by_side =
                    cx.global::<crate::settings::Settings>().diff_layout == crate::settings::DiffLayout::SideBySide;
                let count = if side_by_side { diff.split.len() } else { diff.rows.len() };
                uniform_list(
                    "diff-rows",
                    count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                        let palette = this.palette;
                        let (Some(key), Some(path)) = (this.tabs.active(), this.active_path()) else {
                            return Vec::new();
                        };
                        let Some(Opened { diff: Load::Ready(diff), syntax, .. }) = this.opened.get(key) else {
                            return Vec::new();
                        };
                        let referenced = this.composer.as_ref().map(|c| c.reference.clone());
                        let start = range.start;
                        if side_by_side {
                            let (diff, syntax) = (diff.clone(), syntax.clone());
                            return diff.split[range.start.min(diff.split.len())..range.end.min(diff.split.len())]
                                .iter()
                                .enumerate()
                                .map(|(offset, row)| {
                                    this.render_split_row(start + offset, row, &syntax, &path, referenced.as_ref(), palette, cx)
                                })
                                .collect();
                        }
                        diff.rows[range]
                            .iter()
                            .enumerate()
                            .map(|(offset, row)| {
                                let Row::Line(line) = row else {
                                    return render_row(row, None, palette, false);
                                };
                                let reference = line_reference(&path, line);
                                let is_referenced = referenced.as_ref() == Some(&reference);
                                div()
                                    .id(("diff-row", start + offset))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        // Working in a preview tab keeps it.
                                        if let Some(key) = this.tabs.active().map(str::to_owned) {
                                            this.tabs.keep(&key);
                                        }
                                        this.open_composer(reference.clone(), window, cx)
                                    }))
                                    .child(render_row(row, syntax.spans(line), palette, is_referenced))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .track_scroll(opened.expect("a ready diff is open").scroll.clone())
                .flex_1()
                .into_any_element()
            }
        };

        pane.child(self.render_tab_bar(cx))
            .child(header)
            .child(body)
            .children(self.render_composer(cx))
    }
}

impl Render for OnlyDiffs {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.palette = Palette::current(window.appearance(), cx);
        let palette = self.palette;
        // With the composer gone nothing holds focus, and the tab keys need
        // something inside the window to start from.
        // The agent menu hangs off a footer status that only shows while
        // tied or while nothing here is connected; when that goes, so does
        // the menu.
        let has_trigger = self.tie.is_some()
            || matches!(
                summary::overall(&[
                    (Agent::Claude, self.summary(Agent::Claude)),
                    (Agent::Codex, self.summary(Agent::Codex)),
                ]),
                summary::Overall::NoneConnected(_) | summary::Overall::Unreachable(_)
            );
        if self.context_menu.is_none() && self.context_menu_focus.is_focused(window) {
            window.focus(&self.focus);
        }
        if self.agent_menu && !has_trigger {
            self.agent_menu = false;
        }
        if window.focused(cx).is_none()
            || (!self.agent_menu && self.agent_menu_focus.is_focused(window))
        {
            window.focus(&self.focus);
        }
        div()
            .key_context(PANE_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(Self::close_other_tabs))
            .on_action(cx.listener(Self::stage_active_file))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .on_action(cx.listener(|this, _: &ActivateTab1, _, cx| this.activate_tab(0, cx)))
            .on_action(cx.listener(|this, _: &ActivateTab2, _, cx| this.activate_tab(1, cx)))
            .on_action(cx.listener(|this, _: &ActivateTab3, _, cx| this.activate_tab(2, cx)))
            .on_action(cx.listener(|this, _: &ActivateTab4, _, cx| this.activate_tab(3, cx)))
            .on_action(cx.listener(|this, _: &ActivateTab5, _, cx| this.activate_tab(4, cx)))
            .on_action(cx.listener(|this, _: &ActivateTab6, _, cx| this.activate_tab(5, cx)))
            .on_action(cx.listener(|this, _: &ActivateTab7, _, cx| this.activate_tab(6, cx)))
            .on_action(cx.listener(|this, _: &ActivateTab8, _, cx| this.activate_tab(7, cx)))
            .on_action(cx.listener(|this, _: &ActivateTab9, _, cx| this.activate_tab(8, cx)))
            .on_action(cx.listener(|this, _: &ActivateLastTab, _, cx| {
                let last = this.tabs.len().saturating_sub(1);
                this.activate_tab(last, cx)
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.background)
            .font_family(palette.ui_font)
            .text_sm()
            .line_height(relative(1.4))
            .text_color(palette.text)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_sidebar(cx))
                    .child(self.render_main(cx)),
            )
            .child(self.render_footer(cx))
            .children(self.render_context_menu(cx))
    }
}

/// One agent in the footer: a dot for the state, a label, and the full
/// explanation — including the command that fixes it — on hover.
fn tone_color(tone: Tone, palette: Palette) -> Hsla {
    match tone {
        Tone::Connected => palette.added_text,
        Tone::Unreachable => palette.warning,
        Tone::Error => palette.danger,
        Tone::Off | Tone::Pending => palette.muted,
    }
}

/// The message box's hint: what the message will be about.
fn placeholder(agent: Agent, reference: &LineReference) -> String {
    let about = if reference.is_whole_file() { "this file" } else { "this line" };
    format!("Message {} about {about}…", agent.name())
}

/// What a click on a diff line refers to: the new-file line number, or the
/// old one for a removed line, which has no number in the new file.
fn line_reference(path: &str, line: &Line) -> LineReference {
    let removed = line.kind == RowKind::Removed;
    LineReference {
        path: path.to_owned(),
        line: Some(if removed { line.old_number } else { line.new_number }.unwrap_or(0)),
        removed,
        text: line.text.clone(),
    }
}

/// The logo for an agent, tinted: Claude in its own orange, Codex in the
/// text color so it reads in light and dark.
fn agent_logo(agent: Agent, palette: Palette) -> impl IntoElement {
    let (path, color) = match agent {
        Agent::Claude => ("icons/claude.svg", gpui::rgb(0xD97757).into()),
        Agent::Codex => ("icons/codex.svg", palette.text),
    };
    svg().path(path).size(px(13.)).flex_none().text_color(color)
}

/// "Claude Connected", with where messages go on hover.
fn connected_item(agent: Agent, label: String, detail: String, palette: Palette) -> gpui::Stateful<gpui::Div> {
    let id = match agent {
        Agent::Claude => "agent-claude",
        Agent::Codex => "agent-codex",
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1p5()
        .child(agent_logo(agent, palette))
        .child(div().text_color(palette.text).child(label))
        .with_tip(detail, palette)
}

/// A single word for why nothing is connected, with the reasons — and the
/// commands that fix them — on hover.
fn status_item(
    id: &'static str,
    label: &'static str,
    color: Hsla,
    detail: String,
    palette: Palette,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1p5()
        .child(div().size(px(7.)).rounded_full().bg(color))
        .child(div().text_color(color).child(label))
        .with_tip(detail, palette)
}

/// Where the updater is, in the footer's right corner: progress while it
/// works, then "Restart to Update". Nothing while idle.
fn update_item(cx: &mut Context<OnlyDiffs>, palette: Palette) -> Option<gpui::AnyElement> {
    use crate::update::Status;
    let quiet = |text: String, color: Hsla| Some(div().text_color(color).child(text).into_any_element());
    match crate::update::status(cx) {
        Status::Idle => None,
        Status::Checking => quiet("Checking for Updates…".into(), palette.muted),
        Status::UpToDate => {
            quiet(format!("onlydiffs {} is up to date", crate::update::current_version()), palette.muted)
        }
        Status::Errored(error) => quiet(error, palette.danger),
        Status::Downloading { version, progress } => quiet(
            match progress {
                Some(p) => format!("Downloading {version}… {:.0}%", p * 100.),
                None => format!("Downloading {version}…"),
            },
            palette.muted,
        ),
        Status::Installing { version } => quiet(format!("Installing {version}…"), palette.muted),
        Status::Updated { version } => Some(
            div()
                .id("restart-to-update")
                .px_1p5()
                .mx_neg_1p5()
                .rounded_sm()
                .cursor_pointer()
                .text_color(palette.accent)
                .hover(|item| item.bg(palette.hover))
                .child("Restart to Update")
                .with_tip(format!("onlydiffs {version} is installed. Restart to use it."), palette)
                .on_click(|_, _, cx| crate::update::restart(cx))
                .into_any_element(),
        ),
    }
}

trait WithTip {
    fn with_tip(self, detail: String, palette: Palette) -> Self;
}

impl WithTip for gpui::Stateful<gpui::Div> {
    fn with_tip(self, detail: String, palette: Palette) -> Self {
        if detail.is_empty() {
            return self;
        }
        let detail: SharedString = detail.into();
        self.tooltip(move |_, cx| {
            let detail = detail.clone();
            cx.new(|_| Tip { text: detail, palette }).into()
        })
    }
}

pub struct Tip {
    text: SharedString,
    palette: Palette,
}

impl Tip {
    pub fn new(text: SharedString, palette: Palette) -> Self {
        Self { text, palette }
    }
}

impl Render for Tip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .max_w(px(460.))
            .px_2()
            .py_1p5()
            .rounded_md()
            .border_1()
            .border_color(self.palette.menu_border)
            .bg(self.palette.sidebar)
            // A tooltip is drawn outside the window's tree, so it inherits
            // nothing from the root.
            .font_family(self.palette.ui_font)
            .text_sm()
            .line_height(relative(1.4))
            .text_color(self.palette.text)
            .child(self.text.clone())
    }
}

fn render_row(
    row: &Row,
    spans: Option<&[crate::syntax::Span]>,
    palette: Palette,
    is_referenced: bool,
) -> gpui::AnyElement {
    match row {
        Row::Fold { hidden } => div()
            .h(px(ROW_HEIGHT))
            .px_4()
            .flex()
            .items_center()
            .bg(palette.fold_bg)
            .text_color(palette.muted)
            .child(format!(
                "⋯ {hidden} unchanged line{}",
                if *hidden == 1 { "" } else { "s" }
            ))
            .into_any_element(),
        Row::Line(line) => {
            let background = match line.kind {
                RowKind::Context => None,
                RowKind::Added => Some(palette.added_bg),
                RowKind::Removed => Some(palette.removed_bg),
            };
            let number = |value: Option<u32>| {
                div()
                    .w(px(NUMBER_WIDTH))
                    .flex_none()
                    .pr_2()
                    .flex()
                    .justify_end()
                    .text_color(palette.muted)
                    .child(value.map(|n| n.to_string()).unwrap_or_default())
            };
            div()
                .h(px(ROW_HEIGHT))
                .flex()
                .items_center()
                .font_family(palette.mono_font)
                .whitespace_nowrap()
                .overflow_hidden()
                .when_some(background, |el, bg| el.bg(bg))
                .when(is_referenced, |el| el.bg(palette.selected))
                .when(!is_referenced, |el| el.hover(|el| el.bg(palette.hover)))
                .child(number(line.old_number))
                .child(number(line.new_number))
                .child(change_bar(line.kind, palette))
                .child(div().flex_1().child(highlighted(line, spans, palette)))
                .into_any_element()
        }
    }
}

/// A diff line's text: syntax colours, and the changed words marked with a
/// stronger tint of the line's own colour — what Pierre's `word-alt` does.
/// The two overlap, so the text is cut at every boundary of either and
/// each piece takes both. Tabs are drawn four wide, so every range moves
/// right three for each tab before it.
fn highlighted(line: &Line, spans: Option<&[crate::syntax::Span]>, palette: Palette) -> gpui::StyledText {
    let text = line.text.as_str();
    let shown = expand_tabs(text);
    let spans = spans.unwrap_or(&[]);
    let mark = match line.kind {
        RowKind::Added => Some(Hsla { a: 0.32, ..palette.added_text }),
        RowKind::Removed => Some(Hsla { a: 0.32, ..palette.removed_text }),
        RowKind::Context => None,
    };
    if spans.is_empty() && (line.changes.is_empty() || mark.is_none()) {
        return gpui::StyledText::new(shown);
    }
    let mut cuts: Vec<usize> = vec![0, text.len()];
    for span in spans.iter().filter(|s| s.range.end <= text.len()) {
        cuts.extend([span.range.start, span.range.end]);
    }
    for change in line.changes.iter().filter(|c| c.end <= text.len()) {
        cuts.extend([change.start, change.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    let shift = |at: usize| at + 3 * text[..at].matches('\t').count();
    let runs: Vec<_> = cuts
        .windows(2)
        .filter_map(|pair| {
            let (start, end) = (pair[0], pair[1]);
            let syntax = spans
                .iter()
                .find(|s| s.range.start <= start && end <= s.range.end)
                .map(|s| crate::syntax::theme::style(s.id, palette.icons));
            let marked = mark.filter(|_| line.changes.iter().any(|c| c.start <= start && end <= c.end));
            if syntax.is_none() && marked.is_none() {
                return None;
            }
            let mut style = syntax.unwrap_or_default();
            style.background_color = marked;
            Some((shift(start)..shift(end), style))
        })
        .collect();
    gpui::StyledText::new(shown).with_highlights(runs)
}

/// The change bar at the left of a diff line, as Pierre draws it in place of
/// a + or −: green for added, red for removed, nothing for unchanged.
fn change_bar(kind: RowKind, palette: Palette) -> impl IntoElement {
    let color = match kind {
        RowKind::Added => Some(palette.added_text),
        RowKind::Removed => Some(palette.removed_text),
        RowKind::Context => None,
    };
    div()
        .w(px(16.))
        .h_full()
        .flex_none()
        .child(div().w(px(3.)).h_full().when_some(color, |bar, color| bar.bg(color)))
}

/// GPUI draws a tab as a single narrow glyph; four spaces reads like an editor.
fn expand_tabs(text: &str) -> SharedString {
    if text.contains('\t') {
        text.replace('\t', "    ").into()
    } else {
        SharedString::from(text.to_owned())
    }
}

fn message_block(message: SharedString, color: Hsla) -> gpui::Div {
    div().p_4().text_color(color).child(message)
}

fn status_letter(status: ChangeStatus) -> &'static str {
    match status {
        ChangeStatus::Added => "A",
        ChangeStatus::Modified => "M",
        ChangeStatus::Deleted => "D",
        ChangeStatus::Renamed => "R",
        ChangeStatus::Untracked => "U",
    }
}

/// Triage's mark on a file: a filled dot for "needs review", a hollow one
/// for "skim", each saying why on hover.
fn triage_marker(id: &str, verdict: &crate::triage::Verdict, palette: Palette) -> impl IntoElement {
    let mark = div()
        .id(SharedString::from(format!("triage-{id}")))
        .flex_none()
        .size(px(7.))
        .rounded_full();
    let mark = match verdict.level {
        crate::triage::Level::Review => mark.bg(palette.warning),
        crate::triage::Level::Skim => mark.border_1().border_color(palette.muted),
    };
    mark.with_tip(verdict.describe(), palette)
}

/// The file's Catppuccin icon, drawn in its own colours.
fn file_icon(path: &str, palette: Palette) -> impl IntoElement {
    gpui::img(crate::file_icons::icon_for(path, palette.icons)).size(px(16.)).flex_none()
}

fn status_color(status: ChangeStatus, palette: Palette) -> Hsla {
    match status {
        ChangeStatus::Added | ChangeStatus::Untracked => palette.added_text,
        ChangeStatus::Deleted => palette.removed_text,
        ChangeStatus::Modified | ChangeStatus::Renamed => palette.modified,
    }
}
