//! The settings window: the diff layout, the theme, and the two fonts.
//!
//! Every choice is a dropdown that applies and saves the moment it is
//! picked; there is no OK button. Opened with ⌘, or from the app menu, and
//! only ever once — asking again brings the open window forward.

use gpui::{
    AnyWindowHandle, App, AppContext, Bounds, Context, Corner, FocusHandle, Global, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowOptions, actions, anchored, deferred,
    div, prelude::FluentBuilder, px, relative, size,
};

use crate::settings::{self, DiffLayout, FontChoice, MONO_FONTS, Settings, ThemeChoice, UI_FONTS};
use crate::theme::Palette;

actions!(settings_window, [OpenSettings, CloseSettings, CloseDropdown]);

const CONTEXT: &str = "SettingsWindow";

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-w", CloseSettings, Some(CONTEXT)),
        KeyBinding::new("escape", CloseDropdown, Some(CONTEXT)),
    ]
}

/// The open settings window, so ⌘, brings it forward instead of opening a
/// second one.
#[derive(Default)]
struct Open(Option<AnyWindowHandle>);

impl Global for Open {}

pub fn open(cx: &mut App) {
    if let Some(handle) = cx.try_global::<Open>().and_then(|open| open.0)
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let bounds = Bounds::centered(None, size(px(560.), px(330.)), cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("Settings".into()), ..Default::default() }),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| SettingsView::new(window, cx)),
    );
    if let Ok(handle) = opened {
        cx.set_global(Open(Some(handle.into())));
        cx.activate(true);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dropdown {
    Layout,
    Theme,
    UiFont,
    MonoFont,
}

pub struct SettingsView {
    focus: FocusHandle,
    open: Option<Dropdown>,
    /// The dropdown button under the pointer: its click toggles the list, so
    /// the list's "clicked elsewhere" must leave it alone.
    over_button: Option<Dropdown>,
    _settings: Subscription,
    _appearance: Subscription,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        Self {
            focus,
            open: None,
            over_button: None,
            _settings: cx.observe_global::<Settings>(|_, cx| cx.notify()),
            _appearance: cx.observe_window_appearance(window, |_, _, cx| cx.notify()),
        }
    }

    fn close_dropdown(&mut self, _: &CloseDropdown, _: &mut Window, cx: &mut Context<Self>) {
        self.open = None;
        cx.notify();
    }

    fn close(&mut self, _: &CloseSettings, window: &mut Window, _: &mut Context<Self>) {
        window.remove_window();
    }

    /// One labelled dropdown row.
    fn row(
        &self,
        which: Dropdown,
        label: &'static str,
        current: SharedString,
        current_font: Option<&'static str>,
        options: Vec<(SharedString, Option<&'static str>, bool, Box<dyn Fn(&mut App)>)>,
        palette: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_open = self.open == Some(which);
        let button = div()
            .id(SharedString::from(format!("dropdown-{which:?}")))
            .w(px(280.))
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(if is_open { palette.accent } else { palette.menu_border })
            .bg(palette.background)
            .cursor_pointer()
            .hover(|button| button.bg(palette.hover))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when_some(current_font, |label, font| label.font_family(font))
                    .child(current),
            )
            .child(div().text_color(palette.muted).child("▾"))
            .on_hover(cx.listener(move |this, hovered: &bool, _, _| {
                if *hovered {
                    this.over_button = Some(which);
                } else if this.over_button == Some(which) {
                    this.over_button = None;
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open = if this.open == Some(which) { None } else { Some(which) };
                cx.notify();
            }));

        let list = is_open.then(|| {
            let items = options.into_iter().enumerate().map(|(ix, (text, font, selected, choose))| {
                div()
                    .id(SharedString::from(format!("dropdown-{which:?}-{ix}")))
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
                    .child(div().w(px(12.)).text_color(palette.accent).child(if selected { "✓" } else { "" }))
                    .child(div().when_some(font, |label, font| label.font_family(font)).child(text))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        choose(cx);
                        this.open = None;
                        cx.notify();
                    }))
            });
            div().absolute().top_full().left_0().child(
                deferred(
                    anchored().anchor(Corner::TopLeft).snap_to_window_with_margin(px(8.)).child(
                        div()
                            .id(SharedString::from(format!("dropdown-list-{which:?}")))
                            .occlude()
                            .mt_1()
                            .w(px(280.))
                            .py_1()
                            .flex()
                            .flex_col()
                            .rounded_md()
                            .border_1()
                            .border_color(palette.menu_border)
                            .bg(palette.sidebar)
                            .shadow_lg()
                            .on_mouse_down_out(cx.listener(move |this, _, _, cx| {
                                if this.over_button != Some(which) {
                                    this.open = None;
                                    cx.notify();
                                }
                            }))
                            .children(items),
                    ),
                )
                .with_priority(1),
            )
        });

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .py_2()
            .child(div().text_color(palette.text).child(label))
            .child(div().relative().child(button).children(list))
    }

    fn fonts(
        list: &'static [FontChoice],
        current: &str,
        apply: fn(&mut Settings, &str),
        cx: &App,
    ) -> Vec<(SharedString, Option<&'static str>, bool, Box<dyn Fn(&mut App)>)> {
        settings::available(list, cx)
            .into_iter()
            .map(|choice| {
                let family = choice.family;
                let action: Box<dyn Fn(&mut App)> =
                    Box::new(move |cx: &mut App| settings::update(cx, |s| apply(s, family)));
                (SharedString::from(choice.label), Some(family), choice.family == current, action)
            })
            .collect()
    }
}

fn section(title: &'static str, palette: Palette) -> impl IntoElement {
    div()
        .pt_4()
        .pb_1()
        .text_color(palette.muted)
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(title)
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Palette::current(window.appearance(), cx);
        let settings = cx.global::<Settings>().clone();
        let layouts = DiffLayout::ALL
            .into_iter()
            .map(|layout| {
                let action: Box<dyn Fn(&mut App)> =
                    Box::new(move |cx: &mut App| settings::update(cx, |s| s.diff_layout = layout));
                (SharedString::from(layout.label()), None, layout == settings.diff_layout, action)
            })
            .collect();
        let themes = ThemeChoice::ALL
            .into_iter()
            .map(|theme| {
                let action: Box<dyn Fn(&mut App)> =
                    Box::new(move |cx: &mut App| settings::update(cx, |s| s.theme = theme));
                (SharedString::from(theme.label()), None, theme == settings.theme, action)
            })
            .collect();
        let ui_font = Settings::resolve(UI_FONTS, &settings.ui_font, cx);
        let mono_font = Settings::resolve(MONO_FONTS, &settings.mono_font, cx);
        let label_of = |list: &'static [FontChoice], family: &str| {
            list.iter().find(|c| c.family == family).map_or(family.to_owned(), |c| c.label.to_owned())
        };
        let ui_fonts = Self::fonts(UI_FONTS, ui_font, |s, f| s.ui_font = f.to_owned(), cx);
        let mono_fonts = Self::fonts(MONO_FONTS, mono_font, |s, f| s.mono_font = f.to_owned(), cx);

        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close_dropdown))
            .on_action(cx.listener(Self::close))
            .size_full()
            .px_6()
            .py_3()
            .flex()
            .flex_col()
            .bg(palette.background)
            .text_color(palette.text)
            .font_family(palette.ui_font)
            .text_sm()
            .line_height(relative(1.4))
            .child(section("Diffs", palette))
            .child(self.row(
                Dropdown::Layout,
                "Layout",
                settings.diff_layout.label().into(),
                None,
                layouts,
                palette,
                cx,
            ))
            .child(section("Appearance", palette))
            .child(self.row(
                Dropdown::Theme,
                "Theme",
                settings.theme.label().into(),
                None,
                themes,
                palette,
                cx,
            ))
            .child(self.row(
                Dropdown::UiFont,
                "Interface font",
                label_of(UI_FONTS, ui_font).into(),
                Some(ui_font),
                ui_fonts,
                palette,
                cx,
            ))
            .child(self.row(
                Dropdown::MonoFont,
                "Code font",
                label_of(MONO_FONTS, mono_font).into(),
                Some(mono_font),
                mono_fonts,
                palette,
                cx,
            ))
    }
}
