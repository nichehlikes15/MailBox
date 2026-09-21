use crate::app::AppState;
use crate::models::Theme;
use gpui::{Context, Entity, Window, div, prelude::*, px, rgb};

pub struct MailTopBar {
    pub state: Entity<AppState>,
    pub theme: Entity<Theme>,
}

impl MailTopBar {
    pub fn new(state: Entity<AppState>, theme: Entity<Theme>, cx: &mut Context<Self>) -> Self {
        cx.observe(&theme, |_, _, cx| cx.notify()).detach();
        Self { theme, state }
    }
}

impl Render for MailTopBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.read(cx).clone();
        let state = self.state.clone();

        div()
            .w_full()
            .h(px(35.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(rgb(theme.surface))
            .child(
                div()
                    .id("back-to-inbox")
                    .cursor_pointer()

                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(theme.text))
                    .text_size(px(15.0))
                    .bg(rgb(theme.background))
                    .mb(px(-1.0))
                    .pb(px(1.0))
                    .child("inbox")
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border))
                    
                    .on_click(move |_event, _window, cx| {
                        state.update(cx, |state, cx| {
                            if state.selected_message.is_some() {
                                state.selected_message = None;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                div()
                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(theme.text_inactive))
                    .text_size(px(15.0))
                    .child("starred")
                    .border_b(px(1.0))
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border)),
            )
            .child(
                div()
                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(theme.text_inactive))
                    .text_size(px(15.0))
                    .child("drafts")
                    .border_b(px(1.0))
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border)),
            )
            .child(
                div()
                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(theme.text_inactive))
                    .text_size(px(15.0))
                    .child("sent")
                    .border_b(px(1.0))
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border)),
            )
            .child(
                div()
                    .h_full()
                    .px(px(18.0))
                    .flex()
                    .items_center()
                    .text_color(rgb(theme.text_inactive))
                    .text_size(px(15.0))
                    .child("trash")
                    .border_b(px(1.0))
                    .border_r(px(1.0))
                    .border_color(rgb(theme.border)),
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
