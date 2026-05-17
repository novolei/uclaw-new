//! iLink QR code binding — fetch QR, poll status.
//! These are standalone async functions, not tied to any running instance.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub const ILINK_BASE_URL: &str = "https://ilinkai.weixin.qq.com";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QrStatusKind {
    Wait,
    Scaned,
    Confirmed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrStatus {
    pub status: QrStatusKind,
    pub bot_token: Option<String>,
    /// account_id extracted from ilink_bot_id field in QR status response.
    pub account_id: Option<String>,
}

/// iLink QR fetch result.
///
/// `qrcode` is the opaque token used as `?qrcode=` in polling requests.
/// `qrcode_img_content` is the value to encode into the QR image shown to the user.
#[derive(Debug, Clone)]
pub struct QrInfo {
    pub qrcode: String,
    pub qrcode_img_content: String,
}

pub async fn fetch_qr(base_url: &str) -> Result<QrInfo> {
    let url = format!("{base_url}/ilink/bot/get_bot_qrcode?bot_type=3");
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .header("iLink-App-ClientVersion", "1")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .json()
        .await?;
    let qrcode = resp["qrcode"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| anyhow!("iLink QR response missing 'qrcode' field"))?;
    // qrcode_img_content is the URL/string to encode as a scannable QR image.
    // Fall back to qrcode itself if the field is absent (graceful degradation).
    let qrcode_img_content = resp["qrcode_img_content"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| qrcode.clone());
    Ok(QrInfo { qrcode, qrcode_img_content })
}

pub async fn poll_qr_status(base_url: &str, qrcode: &str) -> Result<QrStatus> {
    let url = format!("{base_url}/ilink/bot/get_qrcode_status?qrcode={qrcode}");
    let resp: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .header("iLink-App-ClientVersion", "1")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .json()
        .await?;
    let kind = match resp["status"].as_str().unwrap_or("wait") {
        "wait"      => QrStatusKind::Wait,
        "scaned"    => QrStatusKind::Scaned,
        "confirmed" => QrStatusKind::Confirmed,
        "expired"   => QrStatusKind::Expired,
        other       => return Err(anyhow!("Unknown iLink QR status: {other}")),
    };
    Ok(QrStatus {
        status: kind,
        bot_token: resp["bot_token"].as_str().map(String::from),
        account_id: resp["ilink_bot_id"].as_str().map(String::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch_qr_returns_qrcode_and_img_content() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/ilink/bot/get_bot_qrcode?bot_type=3")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"qrcode":"test_qr_abc","qrcode_img_content":"https://example.com/qr.png"}"#)
            .create_async()
            .await;

        let result = fetch_qr(&server.url()).await.unwrap();
        assert_eq!(result.qrcode, "test_qr_abc");
        assert_eq!(result.qrcode_img_content, "https://example.com/qr.png");
    }

    #[tokio::test]
    async fn fetch_qr_falls_back_to_qrcode_when_img_content_absent() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/ilink/bot/get_bot_qrcode?bot_type=3")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"qrcode":"test_qr_abc"}"#)
            .create_async()
            .await;

        let result = fetch_qr(&server.url()).await.unwrap();
        assert_eq!(result.qrcode, "test_qr_abc");
        assert_eq!(result.qrcode_img_content, "test_qr_abc");
    }

    #[tokio::test]
    async fn poll_qr_status_confirmed_extracts_token_and_account_id() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"confirmed","bot_token":"tok999","ilink_bot_id":"acc123"}"#)
            .create_async()
            .await;

        let result = poll_qr_status(&server.url(), "qr1").await.unwrap();
        assert_eq!(result.status, QrStatusKind::Confirmed);
        assert_eq!(result.bot_token, Some("tok999".to_string()));
        assert_eq!(result.account_id, Some("acc123".to_string()));
    }

    #[tokio::test]
    async fn poll_qr_status_expired_has_no_token() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"expired"}"#)
            .create_async()
            .await;

        let result = poll_qr_status(&server.url(), "qr1").await.unwrap();
        assert_eq!(result.status, QrStatusKind::Expired);
        assert!(result.bot_token.is_none());
        assert!(result.account_id.is_none());
    }
}
