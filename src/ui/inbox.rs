//! The inbox: the list of emails for whichever account is selected in the
//! sidebar. It is also in charge of:
//!
//! - loading mail for that account (Gmail or mail.tm temp mail),
//! - listening for new temp mail in real time,
//! - opening an email (fetching the full body for Gmail) and handing it to
//!   the email view.
//!
//! Network calls run on tokio (see runtime.rs); this file stores the handles
//! to those background jobs so it can cancel them when they're not needed.
use std::ops::Range;

use crate::app::{AppState, SidebarEmail};
use crate::models::{
    Email, GoogleAccount, TempEmail, Theme, get_gmail_mail, get_gmail_message, get_mail,
    refresh_token,
};
use crate::ui::EmailView;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use futures_util::StreamExt;
use gpui::{
    AnyElement, Context, Entity, Render, Task, UniformListScrollHandle, Window, div, prelude::*,
    px, rgb, uniform_list,
};
use reqwest_eventsource::{Event, EventSource};
use tokio::sync::mpsc;

// The account currently shown, copied out of `AppState` so background work
// can own it without holding a borrow of the state.
enum Account {
    Temp(TempEmail),
    Google(GoogleAccount),
}

impl Account {
    // A unique string per account, used to tell "is this still the account the
    // user is looking at?" after a background request finishes. Also the key
    // into `AppState::email_cache`.
    fn key(&self) -> String {
        match self {
            Account::Temp(account) => format!("temp:{}", account.id),
            Account::Google(account) => format!("google:{}", account.email),
        }
    }
}

/// Messages from the temp-mail listener (running on tokio) to the UI.
// tokio and gpui can't call into each other directly, so the temp-mail
// listener (on tokio) sends these over a channel and the UI side (on gpui)
// receives them and updates the inbox.
enum TempMailEvent {
    Token(String),
    Emails(Vec<Email>),
    Failed,
}

// About `Task`: `cx.spawn(...)` returns a `Task`, which is gpui's handle to a
// running async job. The important rule: **dropping a `Task` cancels it.**
// So storing a task in a field and later overwriting it (or setting it to
// `None`) automatically cancels the old one. That's how this struct makes
// sure only one listener and one "open email" request run at a time.
pub struct Inbox {
    pub emails: Vec<Email>,
    pub loading: bool,
    pub state: Entity<AppState>,
    pub theme: Entity<Theme>,
    pub email_view: Entity<EmailView>,
    /// The listener for the selected account. Replacing or dropping it cancels
    /// the previous one, including its network work on tokio.
    mail_task: Option<Task<()>>,
    /// The Gmail message currently being opened. Only one at a time: clicking
    /// another email drops (cancels) the previous request.
    open_task: Option<Task<()>>,
    // Which email is loading right now (used to show "Opening…" and to ignore
    // double clicks on the same email).
    opening_message_id: Option<String>,
    // `Account::key()` of the account whose mail is shown. Background results
    // for any other account are thrown away.
    active_account_id: Option<String>,
    // Remembers the list's scroll position, so going into an email and back
    // doesn't jump to the top.
    list_scroll: UniformListScrollHandle,
}

impl Inbox {
    pub fn new(
        state: Entity<AppState>,
        email_view: Entity<EmailView>,
        theme: Entity<Theme>,
        cx: &mut Context<Self>,
    ) -> Inbox {
        // Observers: "run this closure whenever `state` calls `cx.notify()`".
        // Here that means: the user picked a different account (or deleted
        // accounts), so check whether we need to load a different inbox, then
        // re-render ourselves. We must notify ourselves because this view is
        // cached (see the long comment in app.rs).
        cx.observe(&state, |this, _state, cx| {
            this.sync_selected_account(cx);
            cx.notify();
        })
        .detach();
        // Same idea for the theme: re-render when the user switches themes.
        // `.detach()` = keep this subscription alive as long as the view exists.
        cx.observe(&theme, |_, _, cx| cx.notify()).detach();

        let mut inbox = Self {
            emails: Vec::new(),
            loading: false,
            state,
            theme,
            email_view,
            mail_task: None,
            open_task: None,
            opening_message_id: None,
            active_account_id: None,
            list_scroll: UniformListScrollHandle::new(),
        };
        // Load whatever account is already selected when the app starts.
        inbox.sync_selected_account(cx);
        inbox
    }

    fn selected_account(&self, cx: &Context<Self>) -> Option<Account> {
        let state = self.state.read(cx);
        match state.selected_sidebar_email {
            Some(SidebarEmail::Temp(index)) => {
                state.temp_email.get(index).cloned().map(Account::Temp)
            }
            Some(SidebarEmail::Google(index)) => state
                .google_accounts
                .get(index)
                .cloned()
                .map(Account::Google),
            _ => None,
        }
    }

    /// Starts a listener when the selected account changes, or clears the
    /// inbox when nothing is selected.
    /// Called on startup and every time `state` changes. `state` changes for
    /// lots of reasons (token refreshes, emails opened...), so it only restarts
    /// the listener when the selected account is actually different.
    fn sync_selected_account(&mut self, cx: &mut Context<Self>) {
        match self.selected_account(cx) {
            Some(account) => {
                if self.active_account_id.as_deref() != Some(account.key().as_str()) {
                    match account {
                        Account::Temp(account) => self.start_mail_listener(account, cx),
                        Account::Google(account) => self.start_google_listener(account, cx),
                    }
                }
            }
            None => {
                if self.active_account_id.is_some() || !self.emails.is_empty() {
                    self.mail_task = None;
                    self.cancel_open();
                    self.active_account_id = None;
                    self.emails.clear();
                    self.loading = false;
                    self.close_email(cx);
                }
            }
        }
    }

    /// Shared setup when switching to an account: cancel the old listener and
    /// any email being opened, show cached mail straight away.
    /// Setting `mail_task` to `None` drops the old task, which cancels it
    /// (and, through `AbortOnDrop`, the tokio work it was waiting on).
    fn switch_to(&mut self, account_key: &str, cx: &mut Context<Self>) {
        self.mail_task = None;
        self.cancel_open();
        self.active_account_id = Some(account_key.to_string());
        self.emails = self
            .state
            .read(cx)
            .email_cache
            .get(account_key)
            .cloned()
            .unwrap_or_default();
        self.close_email(cx);
        self.loading = true;
        cx.notify();
    }

    /// Loads the latest Gmail messages once (Gmail has no live stream here).
    fn start_google_listener(&mut self, mut account: GoogleAccount, cx: &mut Context<Self>) {
        let account_key = format!("google:{}", account.email);
        self.switch_to(&account_key, cx);

        let io = crate::runtime::spawn(async move {
            // This block runs on tokio. It takes ownership of `account` because
            // `get_gmail_mail` may refresh the access token, and we want the
            // updated account back so we can save the new token.
            let result = get_gmail_mail(&mut account, 25).await;
            (account, result)
        });

        self.mail_task = Some(cx.spawn(async move |this, cx| {
            let result = io.await;

            let _ = this.update(cx, |inbox, cx| {
                // The user may have switched accounts while this was loading.
                // If so, this result is stale: ignore it.
                if inbox.active_account_id.as_deref() != Some(account_key.as_str()) {
                    return;
                }
                inbox.loading = false;

                match result {
                    Ok((account, Ok(emails))) => {
                        inbox.store_google_account(account, cx);
                        inbox.merge_emails(emails);
                    }
                    Ok((_, Err(error))) => eprintln!("Failed to retrieve Gmail: {error:#}"),
                    Err(error) => eprintln!("Gmail task stopped: {error}"),
                }

                cx.notify();
            });
        }));
    }

    /// Temp mail (mail.tm) supports live updates, so this listener keeps running
    /// for as long as the account is selected.
    fn start_mail_listener(&mut self, account: TempEmail, cx: &mut Context<Self>) {
        let account_key = format!("temp:{}", account.id);
        let account_id = account.id.clone();
        self.switch_to(&account_key, cx);

        // The network side (token refresh, fetch, live SSE stream) runs on
        // tokio and sends results back over a channel.
        // A channel is a queue between two tasks: tokio pushes `TempMailEvent`s
        // into `sender`, the gpui task below pulls them out of `receiver`.
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let io = crate::runtime::spawn(temp_mail_listener(account, sender));

        self.mail_task = Some(cx.spawn(async move |this, cx| {
            // Held for the life of this task; dropping it aborts the tokio side.
            let _io = io;

            // `recv()` returns `None` once the tokio side has finished and dropped
            // its `sender`, which ends this loop.
            while let Some(event) = receiver.recv().await {
                let updated = this.update(cx, |inbox, cx| {
                    inbox.handle_temp_event(&account_key, &account_id, event, cx)
                });
                // `update` fails if the Inbox view no longer exists; stop listening.
                if updated.is_err() {
                    break;
                }
            }
        }));
    }

    /// Applies one event from the temp-mail listener to the inbox.
    fn handle_temp_event(
        &mut self,
        account_key: &str,
        account_id: &str,
        event: TempMailEvent,
        cx: &mut Context<Self>,
    ) {
        if self.active_account_id.as_deref() != Some(account_key) {
            return;
        }

        match event {
            TempMailEvent::Token(token) => {
                self.state.update(cx, |state, _cx| {
                    if let Some(saved) = state.temp_email.iter_mut().find(|a| a.id == account_id) {
                        saved.token = token;
                    }
                    state.persist();
                });
            }
            TempMailEvent::Emails(emails) => {
                self.merge_emails(emails);
                self.persist_emails(cx);
                self.loading = false;
            }
            TempMailEvent::Failed => self.loading = false,
        }

        cx.notify();
    }

    /// Called when an email row is clicked.
    ///
    /// The old version started a brand-new, never-cancelled request on every
    /// click, so clicking around a few times left many requests racing each
    /// other (each one with its own TLS handshake and token refresh) and their
    /// results overwriting each other in random order. That's what caused the
    /// crashes and the lag. Now there is only ever one request, in `open_task`.
    fn open_email(&mut self, message_id: &str, cx: &mut Context<Self>) {
        let Some(email) = self
            .emails
            .iter()
            .find(|email| email.id == message_id)
            .cloned()
        else {
            return;
        };

        let google_account = {
            let state = self.state.read(cx);
            match state.selected_sidebar_email {
                Some(SidebarEmail::Google(index)) => state.google_accounts.get(index).cloned(),
                _ => None,
            }
        };

        // Temp mail, or a Gmail message whose body we already fetched.
        // Temp-mail emails only have the preview text, and Gmail emails we've
        // already opened have their body cached, so both can be shown right
        // away with no network request.
        let Some(mut account) = google_account.filter(|_| email.body.is_empty()) else {
            self.cancel_open();
            self.show_email(email, cx);
            return;
        };

        // Double-clicking the same email shouldn't start a second request.
        if self.opening_message_id.as_deref() == Some(message_id) {
            return; // already loading this one
        }

        // Replacing the task drops (cancels) any previous open request.
        // Assigning `open_task` below drops the previous task, which cancels
        // any email that was still loading. So the last click always wins.
        self.opening_message_id = Some(email.id.clone());
        let message_id = email.id.clone();
        let io = crate::runtime::spawn(async move {
            // Runs on tokio. Returns the account too, since the token may have
            // been refreshed.
            let result = get_gmail_message(&mut account, &message_id).await;
            (account, result)
        });

        self.open_task = Some(cx.spawn(async move |this, cx| {
            let result = io.await;

            let _ = this.update(cx, |inbox, cx| {
                // Back on the UI side: apply the result.
                inbox.opening_message_id = None;
                match result {
                    Ok((account, Ok(full_email))) => {
                        inbox.store_google_account(account, cx);
                        // Keep the body so reopening this email is instant.
                        inbox.merge_emails(vec![full_email.clone()]);
                        inbox.show_email(full_email, cx);
                    }
                    Ok((_, Err(error))) => eprintln!("Failed to load Gmail message: {error:#}"),
                    Err(error) => eprintln!("Gmail message task stopped: {error}"),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn cancel_open(&mut self) {
        self.open_task = None;
        self.opening_message_id = None;
    }

    /// Hands the email to the email view and marks it as open in `state`.
    /// `cx.notify()` on state makes MailApp swap the inbox for the email view.
    fn show_email(&mut self, email: Email, cx: &mut Context<Self>) {
        self.email_view
            .update(cx, |view, cx| view.show(Some(email.clone()), cx));
        self.state.update(cx, |state, cx| {
            state.selected_message = Some(email);
            cx.notify();
        });
    }

    /// Clears the email view. No notify here: callers notify when they're done.
    fn close_email(&mut self, cx: &mut Context<Self>) {
        self.email_view.update(cx, |view, cx| view.show(None, cx));
        self.state.update(cx, |state, _cx| {
            state.selected_message = None;
        });
    }

    /// Saves a (possibly token-refreshed) Google account back into state.
    /// Looks it up by email rather than index, so it can't panic if the
    /// account list changed while the request was running.
    /// (The old code did `state.google_accounts[index] = account`, which
    /// panicked if "Delete all" was pressed while a request was running.)
    fn store_google_account(&mut self, account: GoogleAccount, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _cx| {
            if let Some(saved) = state
                .google_accounts
                .iter_mut()
                .find(|saved| saved.email == account.email)
            {
                *saved = account;
            }
        });
    }

    /// Adds new emails to the list and updates existing ones, without throwing
    /// away a body we already fetched (the Gmail list request doesn't include
    /// bodies). Newest first.
    fn merge_emails(&mut self, emails: Vec<Email>) {
        for email in emails {
            if let Some(existing) = self
                .emails
                .iter_mut()
                .find(|existing| existing.id == email.id)
            {
                if existing.body.is_empty() || !email.body.is_empty() {
                    *existing = email;
                }
            } else {
                self.emails.push(email);
            }
        }

        self.emails
            .sort_by(|left, right| right.created_at.cmp(&left.created_at));
    }

    /// Saves this account's emails to the on-disk cache.
    fn persist_emails(&self, cx: &mut Context<Self>) {
        if let Some(account_key) = self.active_account_id.clone() {
            let emails = self.emails.clone();
            self.state.update(cx, |state, _cx| {
                state.email_cache.insert(account_key, emails);
                state.persist();
            });
        }
    }

    /// Builds only the rows in `range`, the ones currently visible on screen.
    /// Called by `uniform_list` in `render` below.
    fn render_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = self.theme.read(cx).clone();

        self.emails[range.start.min(self.emails.len())..range.end.min(self.emails.len())]
            .iter()
            .map(|email| {
                let message_id = email.id.clone();
                // `message_id` (above) is captured by the click handler. We use the
                // email's id rather than its index, so the right email opens even if
                // the list changed in the meantime.
                let is_opening = self.opening_message_id.as_deref() == Some(email.id.as_str());

                div()
                    .id(format!("email-{}", email.id))
                    .w_full()
                    .h(px(52.0))
                    .px(px(24.0))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .cursor_pointer()
                    // `cx.listener(...)` wraps a closure so it gets `&mut Inbox` when the
                    // click happens, which lets the handler call methods on the view.
                    .on_click(cx.listener(move |inbox, _event, _window, cx| {
                        inbox.open_email(&message_id, cx);
                    }))
                    // Sender
                    .child(
                        div()
                            .w(px(220.0))
                            .text_size(px(14.0))
                            .text_color(rgb(theme.text_muted))
                            .child(truncate_text(&sender_name(&email.from), 28)),
                    )
                    // Subject
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .ml_auto()
                            .text_size(px(14.0))
                            .text_color(rgb(theme.text_muted))
                            .child(truncate_text(&email.subject, 48)),
                    )
                    .child(
                        div()
                            .w(px(80.0))
                            .ml(px(6.0))
                            .text_size(px(12.0))
                            .text_color(rgb(0x777777))
                            .child(if is_opening {
                                "Opening…".to_string()
                            } else {
                                email_date(&email.created_at)
                            }),
                    )
                    .into_any_element()
            })
            .collect()
    }
}

/// Runs on tokio. Refreshes the token, loads mail, then listens to mail.tm's
/// live stream and reloads whenever something arrives. Stops when the UI side
/// goes away (channel closed) or the task is aborted.
// Everything in here runs on tokio's threads, never on the UI thread, so
// slow network calls can't freeze the window. It only talks to the UI
// through `sender`.
async fn temp_mail_listener(mut account: TempEmail, sender: mpsc::UnboundedSender<TempMailEvent>) {
    if let Ok(token) = refresh_token(&account).await {
        account.token = token.clone();
        if sender.send(TempMailEvent::Token(token)).is_err() {
            return;
        }
    }

    match get_mail(&account).await {
        Ok(emails) => {
            if sender.send(TempMailEvent::Emails(emails)).is_err() {
                return;
            }
        }
        Err(error) => {
            eprintln!("Failed to retrieve mail: {error:#}");
            let _ = sender.send(TempMailEvent::Failed);
        }
    }

    let url = format!(
        "https://mercure.mail.tm/.well-known/mercure?topic=/accounts/{}",
        account.id
    );
    let request = crate::runtime::http_streaming()
        .get(&url)
        .bearer_auth(&account.token)
        .header(reqwest::header::ACCEPT, "text/event-stream");

    // mail.tm pushes a message over this Server-Sent Events stream whenever
    // new mail arrives; we react by re-fetching the message list.
    let mut events = match EventSource::new(request) {
        Ok(events) => events,
        Err(error) => {
            eprintln!("Failed to open temporary mail stream: {error}");
            return;
        }
    };

    while let Some(event) = events.next().await {
        // The UI side has gone away (account switched or view closed).
        if sender.is_closed() {
            break;
        }

        match event {
            Ok(Event::Open) => {}
            Ok(Event::Message(_)) => match get_mail(&account).await {
                Ok(emails) => {
                    if sender.send(TempMailEvent::Emails(emails)).is_err() {
                        break;
                    }
                }
                Err(error) => eprintln!("Failed to refresh temporary mail: {error:#}"),
            },
            Err(error) => eprintln!("Temporary mail connection error: {error}"),
        }
    }

    events.close();
}

fn sender_name(sender: &str) -> String {
    if let Some((name, _)) = sender.split_once('<') {
        let name = name.trim().trim_matches('"');
        if !name.is_empty() {
            return name.to_string();
        }
    }

    sender
        .split_once('@')
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| sender.to_string())
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let text: String = chars.by_ref().take(max_chars).collect();

    if chars.next().is_some() {
        format!("{text}...")
    } else {
        text
    }
}

fn email_date(value: &str) -> String {
    let date = value
        .parse::<i64>()
        .ok()
        .and_then(|milliseconds| {
            DateTime::from_timestamp_millis(milliseconds).map(|date| date.with_timezone(&Local))
        })
        .or_else(|| {
            DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|date| date.with_timezone(&Local))
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.fZ")
                .ok()
                .and_then(|date| Local.from_local_datetime(&date).single())
        });

    let Some(date) = date else {
        return String::new();
    };

    let now = Local::now();
    if date.date_naive() == now.date_naive() {
        date.format("%-I:%M %p").to_string()
    } else {
        date.format("%-d %b").to_string()
    }
}

// `render` builds a fresh description of the UI every time the view is
// re-rendered (after `cx.notify()`). gpui compares nothing, it just lays
// out and paints what you return, which is why keeping render cheap and
// re-rendering rarely (caching) matters.
impl Render for Inbox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.read(cx).clone();
        let selected_account = self.state.read(cx).selected_sidebar_email;
        let has_selected_account = selected_account.is_some();

        let placeholder = |text: String| {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(0x777777))
                        .child(text),
                )
                .into_any_element()
        };

        let content = if !has_selected_account {
            placeholder("Select an inbox".to_string())
        } else if self.emails.is_empty() {
            placeholder(if self.loading {
                let label = match selected_account {
                    Some(SidebarEmail::Google(index)) => self
                        .state
                        .read(cx)
                        .google_accounts
                        .get(index)
                        .map(|account| account.email.clone())
                        .unwrap_or_else(|| "Gmail".to_string()),
                    Some(SidebarEmail::Temp(index)) => self
                        .state
                        .read(cx)
                        .temp_email
                        .get(index)
                        .map(|account| account.address.clone())
                        .unwrap_or_else(|| "temporary email".to_string()),
                    _ => "inbox".to_string(),
                };
                format!("Loading {label}")
            } else {
                "No messages".to_string()
            })
        } else {
            // Only the rows on screen are built and laid out.
            // `uniform_list` is a virtualised list: every row has the same height,
            // so gpui can work out which rows are on screen and only asks us to
            // build those (via `render_rows`). A normal list builds every row
            // every frame, which gets slow as the email cache grows.
            // `cx.processor(...)` gives the row-building closure `&mut Inbox`.
            uniform_list(
                "email-list",
                self.emails.len(),
                cx.processor(|inbox, range: Range<usize>, _window, cx| {
                    inbox.render_rows(range, cx)
                }),
            )
            .track_scroll(&self.list_scroll)
            .size_full()
            .into_any_element()
        };

        div()
            .size_full()
            .min_h(px(0.0))
            .bg(rgb(theme.background))
            .flex()
            .flex_col()
            .child(div().flex_1().min_h(px(0.0)).w_full().child(content))
    }
}
