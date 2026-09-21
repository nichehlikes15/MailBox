//! Gmail support: signing in with Google (OAuth) and reading messages through
//! the Gmail REST API.
//!
//! All the functions here are `async` and do network calls, so they are run
//! on the tokio runtime (via `crate::runtime::spawn`), never directly on the
//! UI thread.
use anyhow::{Context, Result};
use axum::{Router, extract::Query, response::Html, routing::get};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::StreamExt;
use rand::{Rng, distr::Alphanumeric};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

const CLIENT_ID: &str = "830227318434-7mgfk7bucm5mt9sl8271oevg9bjj6vlu.apps.googleusercontent.com";
const REDIRECT_URI: &str = "http://127.0.0.1:49152/callback";
const SCOPE: &str = "https://www.googleapis.com/auth/gmail.readonly";

// A small server that holds the Google client secret (which must not be
// shipped inside the app) and does the token exchange/refresh for us.
const OAUTH_SERVER: &str = "https://mail-server-production-610b.up.railway.app";

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    pub expires_in: u64,
    pub refresh_token: Option<String>,
    pub scope: String,
    pub token_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)] 
pub struct GoogleAccount {
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

impl GoogleAccount {
    fn from_tokens(tokens: GoogleTokenResponse) -> Result<Self> {
        let refresh_token = tokens
            .refresh_token
            .context("Google did not return a refresh token")?;

        Ok(Self {
            email: String::new(),
            access_token: tokens.access_token,
            refresh_token,
            expires_at: now_unix_seconds() + tokens.expires_in,
        })
    }

    async fn load_email(&mut self) -> Result<()> {
        let access_token = self.ensure_access_token().await?.to_owned();

        let client = crate::runtime::http();

        let response = client
            .get("https://gmail.googleapis.com/gmail/v1/users/me/profile")
            .bearer_auth(access_token)
            .send()
            .await
            .context("Failed to load Google account profile")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            anyhow::bail!("Failed to load Google account profile: {} {}", status, body);
        }

        #[derive(Deserialize)]
        struct Profile {
            #[serde(rename = "emailAddress")]
            email_address: String,
        }

        self.email = response
            .json::<Profile>()
            .await
            .context("Failed to parse Google account profile")?
            .email_address;

        Ok(())
    }

    // Google access tokens expire after about an hour. This returns the current
    // one, or uses the refresh token to get a new one first if it's about to
    // expire. That's why callers pass `&mut GoogleAccount` and save the account
    // back into `AppState` afterwards.
    pub async fn ensure_access_token(&mut self) -> Result<&str> {
        if now_unix_seconds() + 60 < self.expires_at {
            return Ok(&self.access_token);
        }

        let client = crate::runtime::http();

        let response = client
            .post(format!("{}/oauth/refresh", OAUTH_SERVER))
            .json(&serde_json::json!({
                "refresh_token": self.refresh_token,
            }))
            .send()
            .await
            .context("Failed to contact OAuth server")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();

            anyhow::bail!("Token refresh failed: {} {}", status, body);
        }

        let tokens = response
            .json::<GoogleTokenResponse>()
            .await
            .context("Failed to parse token refresh response")?;

        self.access_token = tokens.access_token;
        self.expires_at = now_unix_seconds() + tokens.expires_in;

        Ok(&self.access_token)
    }
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

// Google sign-in flow:
// 1. Start a tiny local web server on 127.0.0.1:49152 (axum).
// 2. Open the Google login page in the user's browser.
// 3. After they approve, Google redirects the browser to our local server
//    with a one-time `code`.
// 4. Swap that code for access + refresh tokens (via OAUTH_SERVER).
// 5. Ask Gmail for the account's email address.
// The local server uses `tokio::spawn`, which only works inside a tokio
// runtime; that's one reason login must be started with `runtime::spawn`.
pub async fn login() -> Result<GoogleAccount> {
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);

    let (sender, receiver) = oneshot::channel::<Result<String>>();

    let sender = Arc::new(tokio::sync::Mutex::new(Some(sender)));

    let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

    let app = Router::new().route(
        "/callback",
        get({
            let sender = sender.clone();

            move |query: Query<CallbackQuery>| {
                let sender = sender.clone();

                async move {
                    if let Some(error) = query.error.as_deref() {
                        if let Some(sender) = sender.lock().await.take() {
                            let _ =
                                sender.send(Err(anyhow::anyhow!("Google OAuth error: {}", error)));
                        }

                        return Html(
                            "<h2>Google login failed.</h2>\
                             <p>You can close this window.</p>"
                                .to_string(),
                        );
                    }

                    let Some(code) = query.code.as_deref() else {
                        if let Some(sender) = sender.lock().await.take() {
                            let _ = sender.send(Err(anyhow::anyhow!(
                                "Google did not return an authorization code."
                            )));
                        }

                        return Html(
                            "<h2>Google login failed.</h2>\
                             <p>No authorization code was returned.</p>"
                                .to_string(),
                        );
                    };

                    if let Some(sender) = sender.lock().await.take() {
                        let _ = sender.send(Ok(code.to_string()));
                    }

                    Html(
                        "<h2>Gmail connected!</h2>\
                         <p>You can close this window and return to Mail Box.</p>"
                            .to_string(),
                    )
                }
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:49152")
        .await
        .context("Failed to start OAuth callback server")?;

    let oauth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth\
        ?client_id={}\
        &redirect_uri={}\
        &response_type=code\
        &scope={}\
        &access_type=offline\
        &prompt=consent\
        &code_challenge={}\
        &code_challenge_method=S256",
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(REDIRECT_URI),
        urlencoding::encode(SCOPE),
        urlencoding::encode(&code_challenge),
    );

    let callback_server = tokio::spawn(async move {
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            let _ = shutdown_receiver.await;
        });

        if let Err(error) = server.await {
            if let Some(sender) = sender.lock().await.take() {
                let _ = sender.send(Err(anyhow::anyhow!(
                    "OAuth callback server failed: {}",
                    error
                )));
            }
        }
    });

    if let Err(error) = webbrowser::open(&oauth_url) {
        callback_server.abort();

        return Err(error).context("Failed to open Google OAuth page");
    }

    let code = match tokio::time::timeout(Duration::from_secs(120), receiver).await {
        Err(_) => {
            let _ = shutdown_sender.send(());
            callback_server.abort();

            return Err(anyhow::anyhow!(
                "Timed out waiting for the Google OAuth callback"
            ));
        }

        Ok(Ok(Ok(code))) => code,

        Ok(Ok(Err(error))) => {
            let _ = shutdown_sender.send(());
            callback_server.abort();

            return Err(error).context("OAuth callback channel closed");
        }

        Ok(Err(error)) => {
            let _ = shutdown_sender.send(());
            callback_server.abort();

            return Err(error).context("OAuth callback task failed");
        }
    };

    let _ = shutdown_sender.send(());
    callback_server.abort();

    let mut account = GoogleAccount::from_tokens(exchange_code(&code, &code_verifier).await?)?;

    account.load_email().await?;

    Ok(account)
}

// PKCE: a random secret we keep, plus its SHA-256 hash (the "challenge")
// that we send to Google. When exchanging the code we prove we started the
// login by sending the original secret. Stops someone who intercepts the
// code from using it.
fn generate_code_verifier() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(64)
        .map(char::from)
        .collect()
}

fn generate_code_challenge(code_verifier: &str) -> String {
    let mut hasher = Sha256::new();

    hasher.update(code_verifier.as_bytes());

    let hash = hasher.finalize();

    URL_SAFE_NO_PAD.encode(hash)
}

async fn exchange_code(code: &str, code_verifier: &str) -> Result<GoogleTokenResponse> {
    let client = crate::runtime::http();

    let response = client
        .post(format!("{}/oauth/token", OAUTH_SERVER))
        .json(&serde_json::json!({
            "code": code,
            "code_verifier": code_verifier,
            "redirect_uri": REDIRECT_URI,
        }))
        .send()
        .await
        .context("Failed to contact OAuth server")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        anyhow::bail!("OAuth server token exchange failed: {} {}", status, body);
    }

    response
        .json::<GoogleTokenResponse>()
        .await
        .context("Failed to parse OAuth server token response")
}

#[derive(Debug, Deserialize)]
struct GmailMessageList {
    messages: Option<Vec<GmailMessageRef>>,
}

#[derive(Debug, Deserialize)]
struct GmailMessageRef {
    id: String,
}

#[derive(Debug, Deserialize)]
struct GmailMessage {
    id: String,
    snippet: Option<String>,
    payload: Option<GmailPayload>,

    #[serde(rename = "internalDate")]
    internal_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GmailPayload {
    #[serde(rename = "mimeType", default)]
    mime_type: Option<String>,
    headers: Option<Vec<GmailHeader>>,
    body: Option<GmailBody>,
    parts: Option<Vec<GmailPayload>>,
}

#[derive(Clone, Debug, Deserialize)]
struct GmailHeader {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct GmailBody {
    data: Option<String>,
}

pub async fn get_gmail_mail(account: &mut GoogleAccount, limit: usize) -> Result<Vec<super::temp_mail::Email>> {
// Loads the newest inbox messages (headers + preview only, no bodies).
// Gmail's list endpoint only returns ids, so each message's details need a
// second request.
pub async fn get_gmail_mail(
    account: &mut GoogleAccount,
    limit: usize,
) -> Result<Vec<super::temp_mail::Email>> {
    let access_token = account.ensure_access_token().await?.to_owned();

    let limit = limit.clamp(1, 25);
    let max_results = limit.to_string();

    let client = crate::runtime::http();

    let response = client
        .get("https://gmail.googleapis.com/gmail/v1/users/me/messages")
        .bearer_auth(&access_token)
        .query(&[("labelIds", "INBOX"), ("maxResults", max_results.as_str())])
        .send()
        .await
        .context("Failed to list Gmail messages")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        anyhow::bail!("Failed to list Gmail messages: {} {}", status, body);
    }

    let list = response
        .json::<GmailMessageList>()
        .await
        .context("Failed to parse Gmail message list")?;

    // Fetch message metadata a few at a time instead of one by one.
    let ids: Vec<String> = list
        .messages
        .unwrap_or_default()
        .into_iter()
        .take(limit)
        .map(|message_ref| message_ref.id)
        .collect();

    let emails = futures_util::stream::iter(ids)
        .map(|id| {
            let access_token = access_token.clone();
            async move {
                let response = client
                    .get(format!(
                        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}",
                        id
                    ))
                    .bearer_auth(&access_token)
                    .query(&[
                        ("format", "metadata"),
                        ("metadataHeaders", "From"),
                        ("metadataHeaders", "Subject"),
                    ])
                    .send()
                    .await
                    .ok()?;

                if !response.status().is_success() {
                    return None;
                }

                response
                    .json::<GmailMessage>()
                    .await
                    .ok()
                    .map(|message| to_email(message, false))
            }
        })
        // Run up to 6 of those requests at the same time (keeping the results
        // in order) instead of one after another, so the inbox loads faster.
        .buffered(6)
        .filter_map(|email| async move { email })
        .collect::<Vec<_>>()
        .await;

    Ok(emails)
}

pub async fn get_gmail_message(account: &mut GoogleAccount, message_id: &str) -> Result<super::temp_mail::Email> {
    let access_token = account.ensure_access_token().await?.to_owned();

    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}",
        message_id
    );

    let response = crate::runtime::http()
        .get(&url)
        .bearer_auth(&access_token)
        .query(&[("format", "full")])
        .send()
        .await
        .context("Failed to load Gmail message")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        anyhow::bail!("Failed to load Gmail message: {} {}", status, body);
    }

    let message = response
        .json::<GmailMessage>()
        .await
        .context("Failed to parse Gmail message")?;

    Ok(to_email(message, true))
}

fn to_email(message: GmailMessage, include_body: bool) -> super::temp_mail::Email {
    let headers = message
        .payload
        .as_ref()
        .and_then(|payload| payload.headers.as_ref())
        .cloned()
        .unwrap_or_default();

    let header = |name: &str| {
        headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.clone())
            .unwrap_or_default()
    };

    let body = if include_body {
        message
            .payload
            .as_ref()
            .map(extract_body)
            .unwrap_or_default()
    } else {
        String::new()
    };

    super::temp_mail::Email {
        id: message.id,
        from: header("From"),
        subject: header("Subject"),
        intro: message.snippet.unwrap_or_default(),
        body,
        seen: true,
        created_at: message.internal_date.unwrap_or_default(),
    }
}

/// Prefer the text/plain part; fall back to text/html (converted to text when
/// displayed), then to whatever part has data.
fn extract_body(payload: &GmailPayload) -> String {
    find_part(payload, Some("text/plain"))
        .or_else(|| find_part(payload, Some("text/html")))
        .or_else(|| find_part(payload, None))
        .unwrap_or_default()
}

// Emails are a tree of "parts" (e.g. multipart/alternative containing a
// text/plain part and a text/html part). This searches the tree for the
// first part of the wanted type that actually has content.
fn find_part(payload: &GmailPayload, mime: Option<&str>) -> Option<String> {
    let mime_matches = match (mime, payload.mime_type.as_deref()) {
        (None, _) => true,
        (Some(wanted), Some(actual)) => actual.eq_ignore_ascii_case(wanted),
        (Some(_), None) => false,
    };

    if mime_matches {
        if let Some(text) = payload
            .body
            .as_ref()
            .and_then(|body| body.data.as_deref())
            .and_then(decode_base64url)
            .filter(|text| !text.trim().is_empty())
        {
            return Some(text);
        }
    }

    payload
        .parts
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find_map(|part| find_part(part, mime))
}

/// Gmail uses base64url, sometimes with `=` padding, which URL_SAFE_NO_PAD
/// rejects — strip it first.
fn decode_base64url(data: &str) -> Option<String> {
    URL_SAFE_NO_PAD
        .decode(data.trim_end_matches('='))
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}
