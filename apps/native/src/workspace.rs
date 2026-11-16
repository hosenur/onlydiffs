//! The window's frame: the project rail down the left edge, and beside it
//! either the open project or the project picker.
//!
//! As in the Tauri app: the rail lists every project in first-opened order
//! (so opening one never moves it), the logo at its top goes to the picker,
//! and settings sit at its foot. Each opened project keeps its own view, so
//! switching away and back finds its tabs as they were.

use std::collections::HashMap;

use gpui::{
    AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement,
    PathPromptOptions, Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Window,
    actions, div, img, prelude::FluentBuilder, px, relative,
};
use onlydiffs_core::services::repository::Repository;

use crate::app::OnlyDiffs;
use crate::projects::{self, Project, Store};
use crate::text_input::{InputColors, TextInput};
use crate::theme::Palette;

actions!(workspace, [OpenProjectPicker, OpenTyped, ClosePicker, ChooseFolder]);

const PICKER_CONTEXT: &str = "ProjectPicker";
const RAIL_WIDTH: f32 = 52.;
const TILE: f32 = 32.;

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-o", OpenProjectPicker, None),
        KeyBinding::new("enter", OpenTyped, Some(PICKER_CONTEXT)),
        KeyBinding::new("escape", ClosePicker, Some(PICKER_CONTEXT)),
    ]
}

pub struct Workspace {
    store: Store,
    projects: Vec<Project>,
    /// The open project's root.
    current: Option<String>,
    /// A view per project opened this session, kept so switching back finds
    /// it as it was.
    views: HashMap<String, Entity<OnlyDiffs>>,
    picker_open: bool,
    input: Entity<TextInput>,
    error: Option<SharedString>,
    focus: FocusHandle,
    _settings: Subscription,
    _appearance: Subscription,
}

impl Workspace {
    /// Opens `initial` when given, else the project used last, else the
    /// picker.
    pub fn new(initial: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = Store::load();
        let palette = Palette::current(window.appearance(), cx);
        let input = cx.new(|cx| TextInput::new("~/Developer/my-project", input_colors(palette), cx));
        let mut this = Self {
            projects: store.local(),
            store,
            current: None,
            views: HashMap::new(),
            picker_open: false,
            input,
            error: None,
            focus: cx.focus_handle(),
            _settings: cx.observe_global::<crate::settings::Settings>(|_, cx| cx.notify()),
            _appearance: cx.observe_window_appearance(window, |_, _, cx| cx.notify()),
        };
        match initial.or_else(|| this.store.most_recent()) {
            Some(root) => this.open_root(root, window, cx),
            None => this.show_picker(&OpenProjectPicker, window, cx),
        }
        this
    }

    /// Shows the project at `root`, opening it first if this session has not.
    fn open_root(&mut self, root: String, window: &mut Window, cx: &mut Context<Self>) {
        self.store.touch(&root);
        self.projects = self.store.local();
        if let Some(previous) = self.current.as_ref().and_then(|path| self.views.get(path)) {
            previous.update(cx, |view, cx| view.set_visible(false, cx));
        }
        let view = match self.views.get(&root) {
            Some(view) => {
                view.update(cx, |view, cx| view.set_visible(true, cx));
                view.clone()
            }
            None => {
                let repo = Repository::local(root.clone().into());
                let view = cx.new(|cx| OnlyDiffs::new(repo, window, cx));
                self.views.insert(root.clone(), view.clone());
                view
            }
        };
        window.set_window_title(&projects::name_of(&root));
        self.current = Some(root);
        self.picker_open = false;
        self.error = None;
        view.update(cx, |view, cx| view.focus(window, cx));
        cx.notify();
    }

    fn show_picker(&mut self, _: &OpenProjectPicker, window: &mut Window, cx: &mut Context<Self>) {
        self.picker_open = true;
        self.error = None;
        window.set_window_title("onlydiffs");
        window.focus(&self.input.read(cx).focus_handle(cx));
        cx.notify();
    }

    /// Back to the open project; with none open there is nowhere to go back to.
    fn close_picker(&mut self, _: &ClosePicker, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.current.clone() else { return };
        self.picker_open = false;
        window.set_window_title(&projects::name_of(&root));
        if let Some(view) = self.views.get(&root) {
            view.update(cx, |view, cx| view.focus(window, cx));
        }
        cx.notify();
    }

    fn open_typed(&mut self, _: &OpenTyped, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.input.read(cx).text().to_owned();
        match projects::resolve(&typed) {
            Ok(root) => self.open_root(root, window, cx),
            Err(error) => {
                self.error = Some(error.into());
                cx.notify();
            }
        }
    }

    /// The system folder chooser.
    fn choose_folder(&mut self, _: &ChooseFolder, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = chosen.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            this.update_in(cx, |this, window, cx| match projects::resolve(&path.to_string_lossy()) {
                Ok(root) => this.open_root(root, window, cx),
                Err(error) => {
                    this.error = Some(error.into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn forget(&mut self, root: &str, cx: &mut Context<Self>) {
        self.store.forget(root);
        self.projects = self.store.local();
        // The open project stays open; it returns to the list next time it is
        // opened.
        if self.current.as_deref() != Some(root) {
            self.views.remove(root);
        }
        cx.notify();
    }

    fn render_rail(&self, palette: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let picker_open = self.picker_open || self.current.is_none();
        let tiles = self.projects.iter().map(|project| {
            let is_current = !picker_open && self.current.as_deref() == Some(project.path.as_str());
            let root = project.path.clone();
            div()
                .id(SharedString::from(format!("rail-{}", project.path)))
                .size(px(TILE + 6.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_lg()
                .border_2()
                .border_color(if is_current { palette.accent } else { gpui::transparent_black() })
                .cursor_pointer()
                .hover(|tile| tile.bg(palette.hover))
                .child(project_tile(project, TILE, palette))
                .with_rail_tip(format!("{}\n{}", project.name, project.path), palette)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if this.current.as_deref() != Some(root.as_str()) || this.picker_open {
                        this.open_root(root.clone(), window, cx);
                    }
                }))
        });

        div()
            .w(px(RAIL_WIDTH))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(palette.border)
            .border_r_1()
            .border_color(palette.menu_border)
            .child(
                div()
                    .id("rail-picker")
                    .mt_2()
                    .mb_1()
                    .size(px(TILE + 6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .border_2()
                    .border_color(if picker_open { palette.accent } else { gpui::transparent_black() })
                    .cursor_pointer()
                    .hover(|tile| tile.bg(palette.hover))
                    .child(img("platypus.png").size(px(TILE)).rounded_md())
                    .with_rail_tip("Open another project  ⌘O".into(), palette)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_picker(&OpenProjectPicker, window, cx)
                    })),
            )
            .child(div().w(px(24.)).h_px().my_1().bg(palette.menu_border))
            .child(
                div()
                    .id("rail-projects")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .py_1()
                    .children(tiles),
            )
            .child(
                div()
                    .id("rail-settings")
                    .mb_2()
                    .size(px(TILE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_lg()
                    .text_color(palette.muted)
                    .cursor_pointer()
                    .hover(|tile| tile.bg(palette.hover).text_color(palette.text))
                    .child("⚙")
                    .with_rail_tip("Settings  ⌘,".into(), palette)
                    .on_click(cx.listener(|_, _, _, cx| crate::settings_view::open(cx))),
            )
    }

    fn render_picker(&self, palette: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.projects.iter().map(|project| {
            let root = project.path.clone();
            let forget_root = project.path.clone();
            let is_current = self.current.as_deref() == Some(project.path.as_str());
            div()
                .id(SharedString::from(format!("picker-{}", project.path)))
                .group("picker-row")
                .flex()
                .items_center()
                .gap_3()
                .px_2()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .hover(|row| row.bg(palette.hover))
                .child(project_tile(project, 28., palette))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().truncate().child(project.name.clone()))
                        .child(
                            div()
                                .truncate()
                                .font_family(palette.mono_font)
                                .text_color(palette.muted)
                                .child(project.path.clone()),
                        ),
                )
                .when(is_current, |row| row.child(div().text_color(palette.muted).child("open")))
                .child(
                    div()
                        .id(SharedString::from(format!("forget-{}", project.path)))
                        .flex_none()
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .text_color(palette.muted)
                        .invisible()
                        .group_hover("picker-row", |button| button.visible())
                        .hover(|button| button.bg(palette.selected).text_color(palette.text))
                        .child("✕")
                        .with_rail_tip("Remove from the list (the folder is untouched)".into(), palette)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.forget(&forget_root, cx);
                        })),
                )
                .on_click(cx.listener(move |this, _, window, cx| this.open_root(root.clone(), window, cx)))
        });

        let button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .flex_none()
                .px_3()
                .py_1p5()
                .rounded_md()
                .cursor_pointer()
                .map(|b| {
                    if primary {
                        b.bg(palette.accent).text_color(palette.on_accent)
                    } else {
                        b.border_1().border_color(palette.menu_border).hover(|b| b.bg(palette.hover))
                    }
                })
                .child(label)
        };

        div()
            .key_context(PICKER_CONTEXT)
            .on_action(cx.listener(Self::open_typed))
            .on_action(cx.listener(Self::close_picker))
            .on_action(cx.listener(Self::choose_folder))
            .flex_1()
            .h_full()
            .flex()
            .justify_center()
            .bg(palette.background)
            .child(
                div()
                    .id("picker")
                    .w_full()
                    .max_w(px(580.))
                    .h_full()
                    .overflow_y_scroll()
                    .px_6()
                    .py_10()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(img("platypus.png").size(px(44.)).rounded_lg())
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(div().font_weight(gpui::FontWeight::MEDIUM).child("onlydiffs"))
                                    .child(
                                        div()
                                            .text_color(palette.muted)
                                            .child("See what your agent changed. Talk it through."),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().font_weight(gpui::FontWeight::MEDIUM).child("Open a repository"))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .px_2()
                                            .py_1p5()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(palette.menu_border)
                                            .bg(palette.sidebar)
                                            .font_family(palette.mono_font)
                                            .child(self.input.clone()),
                                    )
                                    .child(button("picker-open", "Open", true).on_click(cx.listener(
                                        |this, _, window, cx| this.open_typed(&OpenTyped, window, cx),
                                    )))
                                    .child(button("picker-choose", "Choose Folder…", false).on_click(
                                        cx.listener(|this, _, window, cx| {
                                            this.choose_folder(&ChooseFolder, window, cx)
                                        }),
                                    )),
                            )
                            .child(div().text_color(palette.muted).child(
                                "Absolute, ~-relative, or relative to your home folder. Any folder inside a checkout opens its repository.",
                            ))
                            .when_some(self.error.clone(), |el, error| {
                                el.child(div().text_color(palette.danger).child(error))
                            }),
                    )
                    .when(!self.projects.is_empty(), |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().pb_1().text_color(palette.muted).child("Projects"))
                                .children(rows),
                        )
                    }),
            )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::current(window.appearance(), cx);
        self.input.update(cx, |input, _| input.set_colors(input_colors(palette)));
        let content = match (&self.current, self.picker_open) {
            (Some(root), false) => self.views.get(root).map(|view| view.clone().into_any_element()),
            _ => None,
        };
        div()
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::show_picker))
            .size_full()
            .flex()
            .bg(palette.background)
            .text_color(palette.text)
            .font_family(palette.ui_font)
            .text_sm()
            .line_height(relative(1.4))
            .child(self.render_rail(palette, cx))
            .child(match content {
                Some(view) => div().flex_1().min_w_0().h_full().child(view).into_any_element(),
                None => self.render_picker(palette, cx).into_any_element(),
            })
    }
}

fn input_colors(palette: Palette) -> InputColors {
    InputColors { text: palette.text, placeholder: palette.muted, cursor: palette.text, selection: palette.selection }
}

/// A project's square: its own artwork, or two letters on a colour picked
/// from its path, as the Tauri app draws them.
fn project_tile(project: &Project, size: f32, palette: Palette) -> gpui::AnyElement {
    if let Some(icon) = &project.icon {
        return div()
            .size(px(size))
            .rounded_md()
            .overflow_hidden()
            .bg(gpui::white())
            .child(img(icon.clone()).size(px(size)))
            .into_any_element();
    }
    let tints = [palette.accent, palette.added_text, palette.modified, palette.removed_text, palette.warning];
    let tint = tints[projects::tint(&project.path, tints.len())];
    div()
        .size(px(size))
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::Hsla { a: 0.18, ..tint })
        .text_color(tint)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(projects::initials(&project.name))
        .into_any_element()
}

/// Tooltips for the rail and picker, in the same style as the rest.
trait RailTip {
    fn with_rail_tip(self, text: String, palette: Palette) -> Self;
}

impl RailTip for gpui::Stateful<gpui::Div> {
    fn with_rail_tip(self, text: String, palette: Palette) -> Self {
        let text: SharedString = text.into();
        self.tooltip(move |_, cx| {
            let text = text.clone();
            cx.new(|_| crate::app::Tip::new(text, palette)).into()
        })
    }
}
