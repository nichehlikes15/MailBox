use gpui::{Context, Entity, Window, WindowControlArea, div, prelude::*, px, rgb, svg};

use crate::app::AppState;
use crate::models::Theme;

// The settings window. It's a separate window with its own root view, so
// it isn't cached and re-renders whenever gpui redraws that window.
pub struct Settings {
    pub theme: Entity<Theme>,
    pub state: Entity<AppState>,
    pub selected_theme: String,
    pub theme_dropdown_open: bool,
}

impl Settings {
    fn selected_theme_label(&self) -> String {
        Theme::available()
            .into_iter()
            .find(|theme| theme.id == self.selected_theme)
            .map(|theme| theme.name)
            .unwrap_or_else(|| "Unknown theme".to_string())
    }

    fn theme_options(&self, theme: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut options = div()
            .id("theme-options")
            .w(px(280.0))
            .mt(px(4.0))
            .p(px(4.0))
            .bg(rgb(theme.surface))
            .border_1()
            .border_color(rgb(theme.border));

        for available_theme in Theme::available() {
            let name = available_theme.id;
            let label = available_theme.name;
            let is_selected = name == self.selected_theme;

            options = options.child(
                div()
                    .id(format!("theme-option-{name}"))
                    .w_full()
                    .px(px(10.0))
                    .py(px(8.0))
                    .text_color(rgb(if is_selected {
                        theme.selected_text
                    } else {
                        theme.text
                    }))
                    .when(is_selected, |this| this.bg(rgb(theme.selected)))
                    .hover(|this| this.bg(rgb(theme.surface_hover)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |settings, _, _, cx| {
                        settings.selected_theme = name.clone();
                        let selected_theme = settings.selected_theme.clone();
                        settings.theme.update(cx, |theme, theme_cx| {
                            // Replace the shared theme and notify. Every view that observes
                            // `theme` (all of them, see their `new()`) then re-renders with the
                            // new colours, in both windows.
                            *theme = Theme::load_named(&selected_theme);
                            theme_cx.notify();
                        });
                        settings.theme_dropdown_open = false;
                        cx.notify();
                    }))
                    .child(label),
            );
        }

        options
    }
}

impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.read(cx).clone();
        let selected_label = self.selected_theme_label();
        let theme_options = self.theme_options(&theme, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text))
            .font_family("Lilex")
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .h(px(35.0))
                    .bg(rgb(theme.surface_hover))
                    .child(
                        div()
                            .h_full()
                            .px(px(18.0))
                            .flex()
                            .items_center()
                            .text_size(px(15.0))
                            .child("Settings"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .window_control_area(WindowControlArea::Drag),
                    )
                    .child(
                        div()
                            .h_full()
                            .flex()
                            .child(
                                div()
                                    .id("settings-minimize-button")
                                    .w(px(46.0))
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(|this| this.bg(rgb(0x303030)))
                                    .window_control_area(WindowControlArea::Min)
                                    .child(
                                        svg()
                                            .path("images/minimize.svg")
                                            .text_color(rgb(theme.text))
                                            .w(px(18.0))
                                            .h(px(18.0)),
                                    ),
                            )
                            .child(
                                div()
                                    .id("settings-maximize-button")
                                    .w(px(46.0))
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(|this| this.bg(rgb(0x303030)))
                                    .window_control_area(WindowControlArea::Max)
                                    .child(if window.is_maximized() {
                                        svg()
                                            .path("images/restore.svg")
                                            .text_color(rgb(theme.text))
                                            .w(px(18.0))
                                            .h(px(18.0))
                                    } else {
                                        svg()
                                            .path("images/maximize.svg")
                                            .text_color(rgb(theme.text))
                                            .w(px(18.0))
                                            .h(px(18.0))
                                    }),
                            )
                            .child(
                                div()
                                    .id("settings-close-button")
                                    .w(px(46.0))
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(|this| this.bg(rgb(0xc42b1c)).text_color(rgb(0xffffff)))
                                    .window_control_area(WindowControlArea::Close)
                                    .child(
                                        svg()
                                            .path("images/close.svg")
                                            .text_color(rgb(theme.text))
                                            .w(px(18.0))
                                            .h(px(18.0)),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .w_full()
                    .child(
                        div()
                            .w(px(220.0))
                            .h_full()
                            .px(px(12.0))
                            .py(px(16.0))
                            .bg(rgb(theme.surface))
                            .child(
                                div()
                                    .px(px(12.0))
                                    .py(px(9.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(theme.selected))
                                    .text_color(rgb(theme.selected_text))
                                    .child("General"),
                            )
                            .child(
                                div()
                                    .px(px(12.0))
                                    .py(px(9.0))
                                    .text_color(rgb(theme.text_inactive))
                                    .child("Appearance"),
                            )
                            .child(
                                div()
                                    .px(px(12.0))
                                    .py(px(9.0))
                                    .text_color(rgb(theme.text_inactive))
                                    .child("Accounts"),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .p(px(32.0))
                            .child(div().text_size(px(17.0)).child("General"))
                            .child(
                                div()
                                    .mt(px(24.0))
                                    .text_size(px(14.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child("General application settings"),
                            )
                            .child(
                                div()
                                    .mt(px(32.0))
                                    .text_size(px(14.0))
                                    .text_color(rgb(theme.text))
                                    .child("Theme"),
                            )
                            .child(
                                div()
                                    .id("theme-selector")
                                    .mt(px(8.0))
                                    .w(px(280.0))
                                    .px(px(10.0))
                                    .py(px(9.0))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .bg(rgb(theme.surface))
                                    .border_1()
                                    .border_color(rgb(theme.border))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|settings, _, _, cx| {
                                        settings.theme_dropdown_open =
                                            !settings.theme_dropdown_open;
                                        cx.notify();
                                    }))
                                    .child(selected_label)
                                    .child("v"),
                            )
                            .when(self.theme_dropdown_open, |this| this.child(theme_options))
                            .child(
                                div()
                                    .mt(px(40.0))
                                    .text_size(px(14.0))
                                    .text_color(rgb(theme.text))
                                    .child("Storage"),
                            )
                            .child(
                                div()
                                    .id("delete-all-button")
                                    .mt(px(10.0))
                                    .w(px(280.0))
                                    .px(px(12.0))
                                    .py(px(10.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(rgb(0x8f2d24))
                                    .text_color(rgb(0xffffff))
                                    .cursor_pointer()
                                    .hover(|this| this.bg(rgb(0xb83a2d)))
                                    .on_click(cx.listener(|settings, _, _, cx| {
                                        settings.state.update(cx, |state, cx| {
                                            state.temp_email.clear();
                                            state.google_accounts.clear();
                                            state.email_cache.clear();
                                            state.selected_email = None;
                                            state.selected_message = None;
                                            state.selected_sidebar_email = None;
                                            state.google_login_status = None;
                                            // The inbox observes `state`, sees nothing is selected any more and
                                            // cancels its background work. Because accounts are now looked up
                                            // by email instead of index, a request that finishes after this
                                            // can't crash the app.
                                            crate::storage::clear();
                                            cx.notify();
                                        });
                                    }))
                                    .child("Delete all saved mail and accounts"),
                            ),
                    ),
            )
    }
}
