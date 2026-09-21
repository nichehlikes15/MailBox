use gpui::{
    App, Context, Entity, StyleRefinement, TitlebarOptions, Window, WindowOptions, div, prelude::*,
    px, rgb, size,
};
use std::collections::HashMap;

use crate::models::{Email, GoogleAccount, TempEmail, Theme};
use crate::ui::{EmailView, Inbox, MailTopBar, Sidebar, TopBar};

pub struct MailApp {
    pub sidebar: Entity<Sidebar>,
    pub topbar: Entity<TopBar>,
    pub mailtopbar: Entity<MailTopBar>,
    pub inbox: Entity<Inbox>,
    pub email_view: Entity<EmailView>,
    pub state: Entity<AppState>,
    pub theme: Entity<Theme>,
}

#[derive(Clone, Debug)]
pub struct AppState {
    pub temp_email: Vec<TempEmail>,
    pub google_accounts: Vec<GoogleAccount>,
    pub email_cache: HashMap<String, Vec<Email>>,
    pub selected_email: Option<usize>,
    pub selected_message: Option<Email>,
    pub selected_sidebar_email: Option<SidebarEmail>,
    pub google_login_status: Option<String>,
}

impl AppState {
    fn from_storage(data: crate::storage::StoredData) -> Self {
        let selected_sidebar_email = if !data.google_accounts.is_empty() {
            Some(SidebarEmail::Google(0))
        } else if !data.temp_email.is_empty() {
            Some(SidebarEmail::Temp(0))
        } else {
            None
        };

        Self {
            temp_email: data.temp_email,
            google_accounts: data.google_accounts,
            email_cache: data.emails,
            selected_email: match selected_sidebar_email {
                Some(SidebarEmail::Temp(index)) => Some(index),
                _ => None,
            },
            selected_message: None,
            selected_sidebar_email,
            google_login_status: None,
        }
    }

    pub fn persist(&self) {
        crate::storage::save(&crate::storage::StoredData {
            temp_email: self.temp_email.clone(),
            google_accounts: self.google_accounts.clone(),
            emails: self.email_cache.clone(),
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarEmail {
    Mail(usize),
    Google(usize),
    Temp(usize),
}

impl MailApp {
    pub fn open(cx: &mut App) {
        let font = include_bytes!("../assets/fonts/Lilex[wght].ttf");

        cx.text_system()
            .add_fonts(vec![std::borrow::Cow::Borrowed(font.as_slice())])
            .expect("Failed to load Lilex font");

        cx.open_window(
            WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                    None,
                    size(px(1200.0), px(800.0)),
                    cx,
                ))),

                titlebar: Some(TitlebarOptions {
                    title: Some("Mail".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),

                ..Default::default()
            },
            |_, cx| {
                let theme = cx.new(|_| Theme::load());
                let state = cx.new(|_| AppState::from_storage(crate::storage::load()));
                let sidebar = cx.new(|cx| Sidebar::new(state.clone(), theme.clone(), cx));
                let topbar = cx.new(|cx| TopBar::new(theme.clone(), state.clone(), cx));
                let mailtopbar = cx.new(|cx| MailTopBar::new(state.clone(), theme.clone(), cx));

                let email_view = cx.new(|cx| EmailView::new(state.clone(), theme.clone(), cx));
                let inbox =
                    cx.new(|cx| Inbox::new(state.clone(), email_view.clone(), theme.clone(), cx));

                cx.new(|_| MailApp {
                    sidebar,
                    topbar,
                    mailtopbar,
                    inbox,
                    email_view,
                    state,
                    theme,
                })
            },
        )
        .unwrap();
    }
}

impl Render for MailApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.read(cx).clone();

        // Child views are cached: each one is only re-rendered when it is
        // notified (they observe the state/theme they display). Without this,
        // any hover or click anywhere rebuilt the entire window, email body
        // included.
        let content = if self.state.read(cx).selected_message.is_some() {
            self.email_view
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element()
        } else {
            self.inbox
                .clone()
                .cached(StyleRefinement::default().size_full())
                .into_any_element()
        };

        div()
            .size_full()
            .bg(rgb(theme.background))
            .text_color(rgb(theme.text_muted))
            .font_family("Lilex")
            .flex()
            .flex_col()
            .child(
                self.topbar.clone().cached(
                    StyleRefinement::default()
                        .w_full()
                        .h(px(35.0))
                        .flex_shrink_0(),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .w_full()
                    .min_h(px(0.0))
                    .flex()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .min_h(px(0.0))
                            // Was missing: without `flex()` the column isn't
                            // a flex container, so the content area never got
                            // a height and nothing inside could scroll.
                            .flex()
                            .flex_col()
                            .child(
                                self.mailtopbar.clone().cached(
                                    StyleRefinement::default()
                                        .w_full()
                                        .h(px(35.0))
                                        .flex_shrink_0(),
                                ),
                            )
                            .child(div().flex_1().w_full().min_h(px(0.0)).child(content)),
                    )
                    .child(
                        self.sidebar.clone().cached(
                            StyleRefinement::default()
                                .w(px(360.0))
                                .h_full()
                                .flex_shrink_0(),
                        ),
                    ),
            )
    }
}
