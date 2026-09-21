//! Temporary email addresses from mail.tm (https://docs.mail.tm).
//!
//! Errors use `anyhow::Result`. The old `Box<dyn std::error::Error>` can't be
//! sent between threads, and these functions run on tokio and hand their
//! results back to the UI thread, so the error type has to be `Send`.
use anyhow::{Context, Result, bail};
use rand::{Rng, distr::Alphanumeric};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Deserialize, Serialize)]
// One email, used for both Gmail and temp mail. `body` is empty until the
// full message has been fetched; `intro` is the short preview.
pub struct Email {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub intro: String,
    pub body: String,
    pub seen: bool,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    #[serde(rename = "hydra:member")]
    messages: Vec<Message>,
}

#[derive(Debug, Deserialize)]
struct Message {
    id: String,
    from: MessageFrom,
    subject: String,
    intro: Option<String>,
    seen: bool,
    #[serde(rename = "createdAt")]
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct MessageFrom {
    address: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TempEmail {
    pub address: String,
    pub password: String,
    pub id: String,
    pub token: String,
}

#[derive(Debug, Deserialize)]
struct AccountResponse {
    id: String,
    address: String,
}

fn random_string(length: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

// Picks mail.tm's first domain, makes up a random username and password,
// creates the account and logs in to get a token.
pub async fn create_account() -> Result<TempEmail> {
    let client = crate::runtime::http();

    let username = random_string(12).to_lowercase();
    let password = random_string(20);

    let domains_response = client.get("https://api.mail.tm/domains").send().await?;
    let domains: serde_json::Value = domains_response.json().await?;
    let domain = domains["hydra:member"][0]["domain"]
        .as_str()
        .context("No mail.tm domain available")?;

    let address = format!("{}@{}", username, domain);

    let response = client
        .post("https://api.mail.tm/accounts")
        .json(&json!({"address": address,"password": password}))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await?;

        bail!("Failed to create account: {} - {}", status, body);
    }

    let token_response = client
        .post("https://api.mail.tm/token")
        .json(&json!({
            "address": address,
            "password": password
        }))
        .send()
        .await?;

    if !token_response.status().is_success() {
        let status = token_response.status();
        let body = token_response.text().await?;

        bail!("Failed to login to Mail.tm: {} - {}", status, body);
    }

    let account: AccountResponse = response.json().await?;

    println!("Temporary email created: {}", account.address);

    println!("Account ID: {}", account.id);

    let token: TokenResponse = token_response.json().await?;

    println!("Mail.tm token acquired");

    Ok(TempEmail {
        address,
        password,
        id: account.id,
        token: token.token,
    })
}

// mail.tm tokens expire, so we log in again to get a fresh one.
async fn get_token(client: &Client, email: &TempEmail) -> Result<String> {
    let response = client
        .post("https://api.mail.tm/token")
        .json(&json!({
            "address": email.address,
            "password": email.password
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("Failed to login to Mail.tm: {} - {}", status, body);
    }

    let token: TokenResponse = response.json().await?;

    Ok(token.token)
}

pub async fn refresh_token(email: &TempEmail) -> Result<String> {
    get_token(crate::runtime::http(), email).await
}

pub async fn get_mail(email: &TempEmail) -> Result<Vec<Email>> {
    let client = crate::runtime::http();
    let token = get_token(client, email).await?;

    let response = client
        .get("https://api.mail.tm/messages")
        .bearer_auth(&token)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("Failed to retrieve mail: {} - {}", status, body);
    }

    let messages: MessagesResponse = response.json().await?;

    let emails = messages
        .messages
        .into_iter()
        .map(|message| Email {
            id: message.id,
            from: message.from.address,
            subject: message.subject,
            intro: message.intro.unwrap_or_default(),
            // mail.tm's message list only includes a preview (`intro`), not the
            // full body, so temp emails currently only show the preview.
            body: String::new(),
            seen: message.seen,
            created_at: message.created_at,
        })
        .collect();

    Ok(emails)
}
