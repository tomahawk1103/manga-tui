use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

pub const DEFAULT_GITHUB_ISSUES_URL: &str = "https://github.com/tomahawk1103/manga-tui/issues/new";
pub const MIN_FEEDBACK_CHARS: usize = 10;
pub const MAX_FEEDBACK_CHARS: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeedbackCategory {
    Suggestion,
    BugReport,
    BrokenProvider,
}

impl FeedbackCategory {
    pub fn issue_prefix(self) -> &'static str {
        match self {
            Self::Suggestion => "[Mangora][Sugerencia]",
            Self::BugReport => "[Mangora][Bug]",
            Self::BrokenProvider => "[Mangora][Proveedor]",
        }
    }

    pub fn issue_label(self) -> &'static str {
        match self {
            Self::Suggestion => "enhancement",
            Self::BugReport | Self::BrokenProvider => "bug",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Suggestion => "suggestion",
            Self::BugReport => "bugReport",
            Self::BrokenProvider => "brokenProvider",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitFeedbackRequest {
    pub category: FeedbackCategory,
    pub message: String,
    pub include_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackDiagnostics {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub ui_language: String,
    pub active_provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackConfigDto {
    pub proxy_configured: bool,
    pub fallback_issues_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitFeedbackResponse {
    pub delivered: bool,
    pub mode: String,
    pub status_message: String,
    pub fallback_issue_url: Option<String>,
}

/// Validates and normalizes an optional feedback proxy URL.
/// Rejects insecure `http://` URLs and explicitly forbids raw Discord webhook URLs inside binaries.
pub fn validate_proxy_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Feedback proxy URL is empty".to_string());
    }
    if !trimmed.starts_with("https://") {
        return Err("Feedback proxy URL must use HTTPS".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("discord.com/api/webhooks") || lower.contains("discordapp.com/api/webhooks") {
        return Err("Raw Discord webhook URLs are forbidden in client binaries; use a rate-limited HTTPS proxy".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn resolve_feedback_proxy_url_from(runtime_env: Option<&str>, compile_env: Option<&str>) -> Option<String> {
    [runtime_env, compile_env]
        .into_iter()
        .flatten()
        .filter_map(|candidate| validate_proxy_url(candidate).ok())
        .next()
}

pub fn resolve_feedback_proxy_url() -> Option<String> {
    let runtime = std::env::var("MANGORA_FEEDBACK_PROXY_URL").ok();
    let compile = option_env!("MANGORA_FEEDBACK_PROXY_URL");
    resolve_feedback_proxy_url_from(runtime.as_deref(), compile)
}

pub fn build_feedback_config() -> FeedbackConfigDto {
    FeedbackConfigDto {
        proxy_configured: resolve_feedback_proxy_url().is_some(),
        fallback_issues_url: DEFAULT_GITHUB_ISSUES_URL.to_string(),
    }
}

pub fn validate_feedback_message(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let count = trimmed.chars().count();
    if count < MIN_FEEDBACK_CHARS {
        return Err(format!("Message is too short ({count}/{MIN_FEEDBACK_CHARS} minimum characters)"));
    }
    if count > MAX_FEEDBACK_CHARS {
        return Err(format!("Message is too long ({count}/{MAX_FEEDBACK_CHARS} maximum characters)"));
    }
    Ok(trimmed.to_string())
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*byte as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn build_github_issue_fallback_url(
    category: FeedbackCategory,
    message: &str,
    diagnostics: Option<&FeedbackDiagnostics>,
) -> String {
    let first_line = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Feedback from Mangora");
    let snippet: String = first_line.chars().take(60).collect();
    let title = format!("{} {}", category.issue_prefix(), snippet);

    let mut body = format!("### Mensaje\n\n{}\n", message.trim());
    if let Some(diag) = diagnostics {
        body.push_str(&format!(
            "\n### Diagnóstico adjunto\n- App: Mangora v{}\n- SO / Arquitectura: {} ({})\n- Idioma UI: {}\n- Proveedor activo: {}\n",
            diag.app_version, diag.os, diag.arch, diag.ui_language, diag.active_provider
        ));
    }

    format!(
        "{DEFAULT_GITHUB_ISSUES_URL}?title={}&labels={}&body={}",
        percent_encode(&title),
        percent_encode(category.issue_label()),
        percent_encode(&body),
    )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProxySubmissionPayload<'a> {
    source: &'static str,
    category: &'static str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostics: Option<&'a FeedbackDiagnostics>,
}

pub async fn submit_feedback_with_endpoint(
    request: SubmitFeedbackRequest,
    diagnostics: FeedbackDiagnostics,
    endpoint: Option<&str>,
) -> Result<SubmitFeedbackResponse, String> {
    let clean_message = validate_feedback_message(&request.message)?;
    let attached_diag = if request.include_diagnostics { Some(&diagnostics) } else { None };
    let fallback_url = build_github_issue_fallback_url(request.category, &clean_message, attached_diag);

    let Some(proxy_raw) = endpoint else {
        return Ok(SubmitFeedbackResponse {
            delivered: false,
            mode: "githubFallback".to_string(),
            status_message: "Direct proxy is not configured; pre-filled GitHub Issue ready to open.".to_string(),
            fallback_issue_url: Some(fallback_url),
        });
    };

    let validated_proxy = if cfg!(test) {
        // Tests run against local httpmock HTTP servers; production strictly requires HTTPS via resolve_feedback_proxy_url.
        proxy_raw.trim().to_string()
    } else {
        validate_proxy_url(proxy_raw)?
    };

    let mut builder = Client::builder().timeout(Duration::from_secs(10));
    if cfg!(test) {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| format!("Could not initialize feedback HTTP client: {e}"))?;

    let payload = ProxySubmissionPayload {
        source: "mangora",
        category: request.category.id(),
        message: &clean_message,
        diagnostics: attached_diag,
    };

    match client.post(&validated_proxy).json(&payload).send().await {
        Ok(resp) if resp.status().is_success() => Ok(SubmitFeedbackResponse {
            delivered: true,
            mode: "proxy".to_string(),
            status_message: "Feedback delivered through secure proxy.".to_string(),
            fallback_issue_url: None,
        }),
        Ok(resp) if resp.status().as_u16() == 429 => Ok(SubmitFeedbackResponse {
            delivered: false,
            mode: "githubFallback".to_string(),
            status_message: "Feedback proxy rate limit reached (429). Please retry later or send via GitHub Issues.".to_string(),
            fallback_issue_url: Some(fallback_url),
        }),
        Ok(resp) => Ok(SubmitFeedbackResponse {
            delivered: false,
            mode: "githubFallback".to_string(),
            status_message: format!("Feedback proxy returned HTTP {}. You can send it directly via GitHub Issues.", resp.status()),
            fallback_issue_url: Some(fallback_url),
        }),
        Err(err) => Ok(SubmitFeedbackResponse {
            delivered: false,
            mode: "githubFallback".to_string(),
            status_message: format!("Could not reach feedback proxy ({err}). You can open GitHub Issues instead."),
            fallback_issue_url: Some(fallback_url),
        }),
    }
}

pub async fn submit_feedback_action(state: &AppState, request: SubmitFeedbackRequest) -> Result<SubmitFeedbackResponse, String> {
    let snap = state.snapshot()?;
    let diagnostics = FeedbackDiagnostics {
        app_version: snap.app_version,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        ui_language: snap.ui_language,
        active_provider: snap.active_provider,
    };
    let endpoint = resolve_feedback_proxy_url();
    submit_feedback_with_endpoint(request, diagnostics, endpoint.as_deref()).await
}

#[cfg(test)]
mod tests {
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use pretty_assertions::assert_eq;

    use super::*;

    fn sample_diag() -> FeedbackDiagnostics {
        FeedbackDiagnostics {
            app_version: "0.1.0".to_string(),
            os: "windows".to_string(),
            arch: "x86_64".to_string(),
            ui_language: "es".to_string(),
            active_provider: "inmanga".to_string(),
        }
    }

    #[test]
    fn validate_feedback_message_enforces_bounds() {
        assert!(validate_feedback_message("   corto   ").is_err());
        let too_long = "a".repeat(MAX_FEEDBACK_CHARS + 1);
        assert!(validate_feedback_message(&too_long).is_err());

        let ok = validate_feedback_message("  InManga falla al cargar capítulos recientes  ").unwrap();
        assert_eq!(ok, "InManga falla al cargar capítulos recientes");
    }

    #[test]
    fn resolve_proxy_rejects_raw_discord_webhooks_and_http() {
        assert_eq!(
            resolve_feedback_proxy_url_from(
                Some("https://discord.com/api/webhooks/12345/secret"),
                Some("https://feedback.mangora.workers.dev/submit"),
            )
            .as_deref(),
            Some("https://feedback.mangora.workers.dev/submit")
        );
        assert!(validate_proxy_url("https://discordapp.com/api/webhooks/1/2").is_err());
        assert!(validate_proxy_url("http://feedback.mangora.workers.dev/submit").is_err());
        assert!(validate_proxy_url("https://feedback.mangora.workers.dev/submit").is_ok());
    }

    #[test]
    fn build_github_issue_fallback_url_includes_title_label_and_optional_diagnostics() {
        let url_with_diag = build_github_issue_fallback_url(
            FeedbackCategory::BrokenProvider,
            "No cargan imágenes en el servidor",
            Some(&sample_diag()),
        );
        assert!(url_with_diag.starts_with(DEFAULT_GITHUB_ISSUES_URL));
        assert!(url_with_diag.contains("labels=bug"));
        assert!(url_with_diag.contains("Mangora%20v0.1.0"));
        assert!(url_with_diag.contains("inmanga"));

        let url_no_diag =
            build_github_issue_fallback_url(FeedbackCategory::Suggestion, "Sería genial poder ordenar favoritos", None);
        assert!(url_no_diag.contains("labels=enhancement"));
        assert!(!url_no_diag.contains("Diagn%C3%B3stico"));
    }

    #[tokio::test]
    async fn submit_feedback_handles_proxy_ok_rate_limit_and_missing_proxy() {
        let no_proxy = submit_feedback_with_endpoint(
            SubmitFeedbackRequest {
                category: FeedbackCategory::Suggestion,
                message: "Me gustaría tener acceso rápido al último capítulo".to_string(),
                include_diagnostics: false,
            },
            sample_diag(),
            None,
        )
        .await
        .unwrap();
        assert!(!no_proxy.delivered);
        assert_eq!(no_proxy.mode, "githubFallback");
        assert!(no_proxy.fallback_issue_url.is_some());

        let server = MockServer::start_async().await;
        let ok_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/submit");
                then.status(200).body("{\"ok\":true}");
            })
            .await;

        let ok_res = submit_feedback_with_endpoint(
            SubmitFeedbackRequest {
                category: FeedbackCategory::BugReport,
                message: "Al abrir el lector offline falla la página 2".to_string(),
                include_diagnostics: true,
            },
            sample_diag(),
            Some(&server.url("/submit")),
        )
        .await
        .unwrap();
        ok_mock.assert_async().await;
        assert!(ok_res.delivered);
        assert_eq!(ok_res.mode, "proxy");
        assert_eq!(ok_res.fallback_issue_url, None);

        let limit_mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/limit");
                then.status(429).body("Too many requests");
            })
            .await;
        let limit_res = submit_feedback_with_endpoint(
            SubmitFeedbackRequest {
                category: FeedbackCategory::BrokenProvider,
                message: "Error 502 en MangaPill desde esta mañana".to_string(),
                include_diagnostics: false,
            },
            sample_diag(),
            Some(&server.url("/limit")),
        )
        .await
        .unwrap();
        limit_mock.assert_async().await;
        assert!(!limit_res.delivered);
        assert_eq!(limit_res.mode, "githubFallback");
        assert!(limit_res.status_message.contains("429"));
        assert!(limit_res.fallback_issue_url.is_some());
    }
}
