use crate::app::AppState;
use crate::models::{Email, Theme};
use gpui::{Context, Entity, Render, SharedString, Window, div, prelude::*, px, rgb, img, svg};

pub struct EmailView {
    pub state: Entity<AppState>,
    pub theme: Entity<Theme>,
    email_id: Option<String>,
    subject: SharedString,
    from: SharedString,
    /// Cleaned-up body text, computed once when the email is shown rather
    /// than on every render.
    body: SharedString,
}

impl EmailView {
    pub fn new(state: Entity<AppState>, theme: Entity<Theme>, cx: &mut Context<Self>) -> Self {
        cx.observe(&theme, |_, _, cx| cx.notify()).detach();

        Self {
            state,
            theme,
            email_id: None,
            subject: SharedString::default(),
            from: SharedString::default(),
            body: SharedString::default(),
        }
    }

    pub fn show(&mut self, email: Option<Email>, cx: &mut Context<Self>) {
        match email {
            Some(email) => {
                let raw = if email.body.trim().is_empty() {
                    &email.intro
                } else {
                    &email.body
                };
                self.body = crate::html_text::display_body(raw).into();
                self.subject = email.subject.clone().into();
                self.from = email.from.clone().into();
                self.email_id = Some(email.id);
            }
            None => {
                self.email_id = None;
                self.subject = SharedString::default();
                self.from = SharedString::default();
                self.body = SharedString::default();
            }
        }
        cx.notify();
    }
}

impl Render for EmailView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.read(cx).clone();
        let Some(email_id) = self.email_id.as_deref() else {
            return div().into_any_element();
        };
        // let state = self.state.clone();

        div()
            .size_full()
            .min_h(px(0.0))
            .px(px(24.0))
            .py(px(20.0))
            .flex()
            .flex_col()
            .bg(rgb(theme.background))
            // .child(
            //     div()
            //         .id("back-to-inbox")
            //         .cursor_pointer()
            //         .text_color(rgb(theme.text))
            //         .on_click(move |_event, _window, cx| {
            //             state.update(cx, |state, cx| {
            //                 state.selected_message = None;
            //                 cx.notify();
            //             });
            //         })
            //         .child("Back to inbox"),
            // )
            .child(
                div()
                    .text_size(px(22.0))
                    .text_color(rgb(theme.text))
                    .child(self.subject.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .mt(px(12.0))
                    .child(
                        img("images/default.png")
                            .w(px(25.0))
                            .h(px(25.0))
                            .rounded_full(),
                    )
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.from.clone()),
                    ),
            )
            .child(
                div()
                    // Keyed by email so each one opens scrolled to the top.
                    .id(format!("email-body-{email_id}"))
                    .mt(px(24.0))
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .pr(px(12.0))
                    .child(
                        div()
                            .text_size(px(15.0))
                            .text_color(rgb(theme.text))
                            .child(self.body.clone()),
                    ),
            )
            .into_any_element()
    }
}
