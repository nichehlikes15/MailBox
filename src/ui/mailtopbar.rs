use crate::app::{AppState, MailFilter};
use crate::models::Theme;
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};

// The tab strip above the inbox and its Gmail label filters.
pub struct MailTopBar {
    pub state: Entity<AppState>,
    pub theme: Entity<Theme>,
}

impl MailTopBar {
    pub fn new(state: Entity<AppState>, theme: Entity<Theme>, cx: &mut Context<Self>) -> Self {
        // Cached view (see app.rs): redraw when the theme changes.
        cx.observe(&theme, |_, _, cx| cx.notify()).detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self { theme, state }
    }
}

impl Render for MailTopBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.read(cx).clone();
        let selected_filter = self.state.read(cx).mail_filter;
        let state = self.state.clone();

        div()
            .w_full()
            .h(px(35.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(rgb(theme.surface))
            .on_mouse_move(move |_event, _window, cx| {
                state.update(cx, |_state, cx| {
                    cx.notify();
                });
            })
            .child(
                div()
                    .id("back-to-inbox")
                    .cursor_pointer()
                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(if selected_filter == MailFilter::Inbox {
                        theme.text
                    } else {
                        theme.text_inactive
                    }))
                    .text_size(px(15.0))
                    .bg(rgb(if selected_filter == MailFilter::Inbox {
                        theme.background
                    } else {
                        theme.surface
                    }))
                    .mb(px(if selected_filter == MailFilter::Inbox { -1.0 } else { 0.0 }))
                    .pb(px(if selected_filter == MailFilter::Inbox { 1.0 } else { 0.0 }))
                    .child("inbox")
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border))
                    .border_b(px(if selected_filter == MailFilter::Inbox { 0.0 } else { 1.0 }))
                    .on_click(cx.listener(|topbar, _event, _window, cx| {
                        topbar.state.update(cx, |state, cx| {
                            state.mail_filter = MailFilter::Inbox;
                            state.selected_message = None;
                            cx.notify();
                        });
                    })),
            )
            .child(
                div()
                    .id("starred-tab")
                    .cursor_pointer()
                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(if selected_filter == MailFilter::Starred {
                        theme.text
                    } else {
                        theme.text_inactive
                    }))
                    .text_size(px(15.0))
                    .bg(rgb(if selected_filter == MailFilter::Starred {
                        theme.background
                    } else {
                        theme.surface
                    }))
                    .mb(px(if selected_filter == MailFilter::Starred { -1.0 } else { 0.0 }))
                    .pb(px(if selected_filter == MailFilter::Starred { 1.0 } else { 0.0 }))
                    .child("starred")
                    .border_b(px(1.0))
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border))
                    .border_b(px(if selected_filter == MailFilter::Starred { 0.0 } else { 1.0 }))
                    .on_click(cx.listener(|topbar, _event, _window, cx| {
                        topbar.state.update(cx, |state, cx| {
                            state.mail_filter = MailFilter::Starred;
                            cx.notify();
                        });
                    })),
            )
            .child(
                div()
                    .id("drafts-tab")
                    .cursor_pointer()
                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(if selected_filter == MailFilter::Drafts {
                        theme.text
                    } else {
                        theme.text_inactive
                    }))
                    .text_size(px(15.0))
                    .bg(rgb(if selected_filter == MailFilter::Drafts {
                        theme.background
                    } else {
                        theme.surface
                    }))
                    .mb(px(if selected_filter == MailFilter::Drafts { -1.0 } else { 0.0 }))
                    .pb(px(if selected_filter == MailFilter::Drafts { 1.0 } else { 0.0 }))
                    .child("drafts")
                    .border_b(px(1.0))
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border))
                    .border_b(px(if selected_filter == MailFilter::Drafts { 0.0 } else { 1.0 }))
                    .on_click(cx.listener(|topbar, _event, _window, cx| {
                        topbar.state.update(cx, |state, cx| {
                            state.mail_filter = MailFilter::Drafts;
                            state.selected_message = None;
                            cx.notify();
                        });
                    })),
            )
            .child(
                div()
                    .id("sent-tab")
                    .cursor_pointer()
                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(if selected_filter == MailFilter::Sent {
                        theme.text
                    } else {
                        theme.text_inactive
                    }))
                    .text_size(px(15.0))
                    .bg(rgb(if selected_filter == MailFilter::Sent {
                        theme.background
                    } else {
                        theme.surface
                    }))
                    .mb(px(if selected_filter == MailFilter::Sent { -1.0 } else { 0.0 }))
                    .pb(px(if selected_filter == MailFilter::Sent { 1.0 } else { 0.0 }))
                    .child("sent")
                    .border_b(px(1.0))
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border))
                    .border_b(px(if selected_filter == MailFilter::Sent { 0.0 } else { 1.0 }))
                    .on_click(cx.listener(|topbar, _event, _window, cx| {
                        topbar.state.update(cx, |state, cx| {
                            state.mail_filter = MailFilter::Sent;
                            state.selected_message = None;
                            cx.notify();
                        });
                    })),
            )
            .child(
                div()
                    .id("trash-tab")
                    .cursor_pointer()
                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(if selected_filter == MailFilter::Trash {
                        theme.text
                    } else {
                        theme.text_inactive
                    }))
                    .text_size(px(15.0))
                    .bg(rgb(if selected_filter == MailFilter::Trash {
                        theme.background
                    } else {
                        theme.surface
                    }))
                    .mb(px(if selected_filter == MailFilter::Trash { -1.0 } else { 0.0 }))
                    .pb(px(if selected_filter == MailFilter::Trash { 1.0 } else { 0.0 }))
                    .child("trash")
                    .border_b(px(1.0))
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border))
                    .border_b(px(if selected_filter == MailFilter::Trash { 0.0 } else { 1.0 }))
                    .on_click(cx.listener(|topbar, _event, _window, cx| {
                        topbar.state.update(cx, |state, cx| {
                            state.mail_filter = MailFilter::Trash;
                            state.selected_message = None;
                            cx.notify();
                        });
                    })),
            )
            .child(
                div()
                    .h_full()
                    .px(px(15.0))
                    .flex()
                    .items_center()
                    .text_size(px(20.0))
                    .text_color(rgb(theme.text_inactive))
                    .child("+")
                    .border_b(px(1.0))
                    .border_color(rgb(theme.border)),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .border_b(px(1.0))
                    .border_color(rgb(theme.border)),
            )
            .into_any_element()
    }
}
