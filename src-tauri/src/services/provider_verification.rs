//! Explicit provider diagnostics. Reachability, stored credentials and inference are separate facts.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    NotTested,
    Reachable,
    Configured,
    SignedIn,
    Verified,
    NotRequired,
    Missing,
    Failed,
    RateLimited,
    Unsupported,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub endpoint: CheckState,
    pub authentication: CheckState,
    pub model: CheckState,
    pub reason: String,
    pub http_status: Option<u16>,
    pub tested_at: i64,
    pub profile_id: Option<String>,
    pub provider_id: String,
}
impl VerificationReport {
    fn new(reason: &str) -> Self {
        Self {
            endpoint: CheckState::NotTested,
            authentication: CheckState::NotTested,
            model: CheckState::NotTested,
            reason: reason.into(),
            http_status: None,
            tested_at: chrono::Utc::now().timestamp_millis(),
            profile_id: None,
            provider_id: String::new(),
        }
    }
}
fn text(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
}
fn output(v: &Value) -> bool {
    if v.get("error").is_some() || v.get("is_error").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    if text(v.get("result")) || text(v.get("output_text")) {
        return true;
    }
    if v.get("type").and_then(Value::as_str) == Some("response.output_text.delta")
        && text(v.get("delta"))
    {
        return true;
    }
    if v.get("type").and_then(Value::as_str) == Some("text") && text(v.pointer("/part/text")) {
        return true;
    }
    if v.pointer("/item/type").and_then(Value::as_str) == Some("agent_message")
        && text(v.pointer("/item/text"))
    {
        return true;
    }
    if let Some(delta) = v.get("delta") {
        if delta.get("type").and_then(Value::as_str) == Some("text_delta")
            && text(delta.get("text"))
        {
            return true;
        }
    }
    for field in ["content", "output", "choices", "candidates"] {
        if let Some(array) = v.get(field).and_then(Value::as_array) {
            for item in array {
                if text(item.get("text"))
                    || text(item.pointer("/message/content"))
                    || text(item.pointer("/delta/content"))
                    || output(item)
                {
                    return true;
                }
                if item
                    .pointer("/content/parts")
                    .and_then(Value::as_array)
                    .is_some_and(|parts| parts.iter().any(|p| text(p.get("text"))))
                {
                    return true;
                }
            }
        }
    }
    v.get("response").is_some_and(output)
}
fn is_error(v: &Value) -> bool {
    v.get("error").is_some()
        || v.get("is_error").and_then(Value::as_bool) == Some(true)
        || matches!(
            v.get("type").and_then(Value::as_str),
            Some("error" | "response.failed" | "turn.failed" | "session.error")
        )
}
pub fn classify_response(status: u16, body: &[u8]) -> VerificationReport {
    let mut report = VerificationReport::new("invalid_response");
    report.endpoint = CheckState::Reachable;
    report.http_status = Some(status);
    if matches!(status, 401 | 403) {
        report.authentication = CheckState::Failed;
        report.model = CheckState::Failed;
        report.reason = if status == 401 {
            "unauthorized"
        } else {
            "access_denied"
        }
        .into();
        return report;
    }
    if status == 429 {
        report.model = CheckState::RateLimited;
        report.reason = "rate_limited".into();
        return report;
    }
    if !(200..300).contains(&status) {
        report.model = CheckState::Failed;
        report.reason = "upstream_error".into();
        return report;
    }
    let source = String::from_utf8_lossy(body);
    let values: Vec<Value> = serde_json::from_slice(body)
        .ok()
        .map(|v| vec![v])
        .unwrap_or_else(|| {
            source
                .lines()
                .filter_map(|line| {
                    serde_json::from_str(line.trim().strip_prefix("data:").unwrap_or(line).trim())
                        .ok()
                })
                .collect()
        });
    if !values.iter().any(is_error) && values.iter().any(output) {
        report.authentication = CheckState::Verified;
        report.model = CheckState::Verified;
        report.reason = "model_verified".into();
    } else {
        report.model = CheckState::Failed;
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn public_catalog_response_does_not_prove_authentication_or_generation() {
        let report = classify_response(200, br#"{"data":[{"id":"mock"}]}"#);
        assert_ne!(report.authentication, CheckState::Verified);
        assert_ne!(report.model, CheckState::Verified);
    }
}

use crate::proxy::{
    handler_context::RequestContext,
    providers::{get_adapter, ClaudeAdapter, ProviderAdapter},
    server::ProxyState,
    ProxyError,
};
use crate::services::launch_profiles::{LaunchProfile, LaunchRuntime, RoutingMode};
use crate::{app_config::AppType, database::Database, provider::Provider};
use serde_json::json;
use std::sync::Arc;
const MAX_BODY: usize = 256 * 1024;
const PROMPT: &str = "Reply exactly CC_SWITCH_CONNECTION_OK. Do not use any tools or access files.";

fn str_at<'a>(value: &'a Value, paths: &[&str]) -> Option<&'a str> {
    paths.iter().find_map(|p| {
        value
            .pointer(p)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    })
}
fn model_for(provider: &Provider) -> Option<String> {
    let c = &provider.settings_config;
    str_at(
        c,
        &[
            "/env/ANTHROPIC_MODEL",
            "/env/GEMINI_MODEL",
            "/model",
            "/defaultModel",
        ],
    )
    .map(str::to_owned)
    .or_else(|| {
        c.get("config")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<toml::Value>().ok())
            .and_then(|v| v.get("model").and_then(|v| v.as_str()).map(str::to_owned))
    })
    .or_else(|| {
        c.get("models")
            .and_then(Value::as_object)
            .and_then(|m| m.iter().next())
            .map(|(id, v)| str_at(v, &["/model", "/id"]).unwrap_or(id).to_owned())
    })
    .or_else(|| {
        c.get("models")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .and_then(|v| str_at(v, &["/id", "/model"]))
            .map(str::to_owned)
    })
}
fn native_required(app: &AppType) -> &'static str {
    if matches!(app, AppType::Claude | AppType::Codex | AppType::OpenCode) {
        "native_profile_required"
    } else {
        "native_auth_unsupported"
    }
}
/// Adapt existing persisted tool schemas to the existing Claude multi-format forwarder.
fn target(app: &AppType, source: &Provider) -> Result<Provider, &'static str> {
    if matches!(source.category.as_deref(), Some("omo" | "omo-slim")) {
        return Err(native_required(app));
    }
    let kind = if source.is_github_copilot() {
        Some("github_copilot")
    } else if source.is_xai_oauth() {
        Some("xai_oauth")
    } else if source.is_codex_oauth() || source.uses_managed_account_auth() {
        Some("codex_oauth")
    } else {
        None
    };
    if matches!(kind, Some("codex_oauth" | "github_copilot" | "xai_oauth")) {
        if source
            .meta
            .as_ref()
            .and_then(|m| m.managed_account_id_for(kind.unwrap()))
            .filter(|s| !s.is_empty())
            .is_none()
        {
            return Err("account_binding_required");
        }
        let mut p = source.clone();
        p.category = None;
        p.meta.get_or_insert_with(Default::default).provider_type = kind.map(str::to_owned);
        if !matches!(app, AppType::Claude | AppType::ClaudeDesktop) {
            p.settings_config = json!({"env":{}});
        }
        return Ok(p);
    }
    if source.category.as_deref() == Some("official") {
        return Err(native_required(app));
    }
    if matches!(app, AppType::Claude | AppType::ClaudeDesktop) {
        if ClaudeAdapter::new()
            .extract_auth(source)
            .is_some_and(|auth| {
                auth.strategy == crate::proxy::providers::AuthStrategy::GoogleOAuth
                    && auth.access_token.is_none()
                    && auth.api_key.trim_start().starts_with('{')
            })
        {
            // Never send a refresh-only credential JSON as a bearer token.
            return Err(native_required(app));
        }
        return Ok(source.clone());
    }
    let mut p = source.clone();
    let mut c = source.settings_config.clone();
    if matches!(app, AppType::OpenCode) {
        c = crate::opencode_config::provider_fragment(
            &source.id,
            &c,
            source.opencode_config_format(),
        )
        .map_err(|_| "unsupported_configuration")?
        .0
        .clone();
    }
    let adapter = if *app == AppType::Agy {
        get_adapter(&AppType::Gemini)
    } else {
        get_adapter(app)
    };
    let base = super::stream_check::StreamCheckService::resolve_base_url(app, source)
        .ok()
        .or_else(|| {
            str_at(
                &c,
                &[
                    "/options/baseURL",
                    "/settings/baseURL",
                    "/baseUrl",
                    "/base_url",
                    "/provider/base_url",
                ],
            )
            .map(str::to_owned)
        })
        .ok_or("endpoint_missing")?;
    let auth = adapter.as_ref().and_then(|a| a.extract_auth(source));
    if auth
        .as_ref()
        .is_some_and(|a| a.strategy == crate::proxy::providers::AuthStrategy::GoogleOAuth)
    {
        return Err(native_required(app));
    }
    let key = auth
        .map(|a| a.api_key)
        .or_else(|| {
            str_at(
                &c,
                &[
                    "/options/apiKey",
                    "/settings/apiKey",
                    "/apiKey",
                    "/api_key",
                    "/provider/api_key",
                ],
            )
            .map(str::to_owned)
        })
        .unwrap_or_default();
    let key = match app {
        AppType::Pi => crate::pi_config::resolve_usage_api_key(&key),
        AppType::Omp => crate::omp_config::resolve_usage_api_key(&key),
        _ => key,
    };
    if key.contains("{env:")
        || key.contains("{file:")
        || key.starts_with('$')
        || c.pointer("/provider/oauth").is_some()
        || c.get("auth").and_then(Value::as_str) == Some("oauth")
    {
        return Err(native_required(app));
    }
    let package = str_at(&c, &["/package", "/npm"]).unwrap_or("");
    let declared = source
        .meta
        .as_ref()
        .and_then(|m| m.api_format.as_deref())
        .or_else(|| str_at(&c, &["/api", "/api_mode", "/provider/type"]));
    let protocol = if package.contains("anthropic") {
        "anthropic"
    } else if package.contains("google") {
        "gemini_native"
    } else if package == "@ai-sdk/openai" || package.ends_with("/openai/responses") {
        "openai_responses"
    } else if matches!(app, AppType::Codex | AppType::GrokBuild) {
        if crate::proxy::providers::should_convert_codex_responses_to_anthropic(
            source,
            "/v1/responses",
        ) {
            "anthropic"
        } else if crate::proxy::providers::should_convert_codex_responses_to_chat(
            source,
            "/v1/responses",
        ) {
            "openai_chat"
        } else {
            "openai_responses"
        }
    } else if matches!(app, AppType::Gemini | AppType::Agy) {
        "gemini_native"
    } else {
        match declared {
            Some("anthropic" | "anthropic-messages" | "anthropic_messages") => "anthropic",
            Some("openai-responses" | "openai_responses" | "responses") => "openai_responses",
            Some("google-generative-ai" | "gemini_native" | "gemini") => "gemini_native",
            Some("openai" | "openai-completions" | "openai_chat" | "chat_completions" | "chat")
            | None => "openai_chat",
            _ => return Err("unsupported_configuration"),
        }
    };
    let meta = p.meta.get_or_insert_with(Default::default);
    meta.api_format = Some(protocol.into());
    meta.api_key_field = if protocol == "anthropic" {
        Some("ANTHROPIC_API_KEY".into())
    } else {
        None
    };
    let headers = c
        .get("headers")
        .or_else(|| c.pointer("/options/headers"))
        .or_else(|| c.pointer("/provider/custom_headers"));
    if let Some(headers) = headers.and_then(Value::as_object) {
        let overrides = meta
            .local_proxy_request_overrides
            .get_or_insert_with(Default::default);
        for (name, value) in headers {
            if let Some(v) = value.as_str() {
                overrides.headers.insert(name.clone(), v.into());
            }
        }
    }
    p.settings_config = json!({"env":{"ANTHROPIC_BASE_URL":base,"ANTHROPIC_AUTH_TOKEN":key,"ANTHROPIC_API_KEY":key}});
    Ok(p)
}

fn failure(error: ProxyError) -> VerificationReport {
    match error {
        ProxyError::UpstreamError { status, .. } => classify_response(status, b""),
        ProxyError::AuthError(_) => {
            let mut r = VerificationReport::new("authentication_failed");
            r.authentication = CheckState::Failed;
            r.model = CheckState::Failed;
            r
        }
        _ => {
            let mut r = VerificationReport::new("request_failed");
            r.model = CheckState::Failed;
            r
        }
    }
}

pub async fn verify_provider(
    db: Arc<Database>,
    handle: Option<tauri::AppHandle>,
    app: AppType,
    provider_id: &str,
    profile_id: Option<&str>,
    model: Option<&str>,
    generate: bool,
) -> Result<VerificationReport, String> {
    let provider = db
        .get_provider_by_id(provider_id, app.as_str())
        .map_err(|_| "Cannot read provider")?
        .ok_or("Provider no longer exists")?;
    let profile = if let Some(id) = profile_id {
        let profile = db
            .list_launch_profiles()
            .map_err(|_| "Cannot read profile")?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or("Profile no longer exists")?;
        if profile.provider_id != provider.id || profile.runtime.as_str() != app.as_str() {
            return Err("Profile does not belong to this provider and CLI".into());
        }
        Some(profile)
    } else {
        None
    };
    // Validate the same private environment and local proxy availability as a real launch.
    // Dispose of the short-lived credential without ever exposing it to the frontend.
    if let Some(profile) = profile
        .as_ref()
        .filter(|p| p.routing_mode == RoutingMode::Proxy)
    {
        match super::launch_profiles::command_by_id(&db, &profile.id).await {
            Ok(command) => {
                if let Some(token) = command
                    .get_envs()
                    .find(|(k, _)| *k == "ANTHROPIC_AUTH_TOKEN")
                    .and_then(|(_, v)| v)
                {
                    db.revoke_launch_session(&super::launch_profiles::digest(
                        token.as_encoded_bytes(),
                    ))
                    .map_err(|_| "Cannot revoke diagnostic credential")?;
                }
            }
            Err(_) => {
                let mut report = VerificationReport::new("profile_unavailable");
                report.endpoint = CheckState::Failed;
                report.profile_id = profile_id.map(str::to_owned);
                report.provider_id = provider.id;
                return Ok(report);
            }
        }
    }
    let result = if let Some(profile) = profile
        .as_ref()
        .filter(|p| p.routing_mode != RoutingMode::Proxy)
    {
        verify_native(&db, profile, model, generate).await
    } else {
        verify_api(
            db.clone(),
            handle,
            &app,
            &provider,
            model.or_else(|| profile.as_ref().and_then(|p| p.model.as_deref())),
            generate,
        )
        .await
    };
    let mut report = result;
    report.profile_id = profile_id.map(str::to_owned);
    report.provider_id = provider.id;
    Ok(report)
}

async fn managed_login(
    handle: Option<&tauri::AppHandle>,
    provider: &Provider,
) -> (CheckState, &'static str) {
    use crate::commands::{CodexOAuthState, CopilotAuthState, XaiOAuthState};
    use tauri::Manager;
    let Some(meta) = provider.meta.as_ref() else {
        return (CheckState::Missing, "no_stored_credentials");
    };
    let Some(kind) = meta.provider_type.as_deref() else {
        return (CheckState::Missing, "no_stored_credentials");
    };
    let Some(id) = meta.managed_account_id_for(kind) else {
        return (CheckState::Missing, "account_binding_required");
    };
    let Some(handle) = handle else {
        return (CheckState::Unsupported, "auth_status_unavailable");
    };
    let needs_reauth = match kind {
        "codex_oauth" => {
            let Some(state) = handle.try_state::<CodexOAuthState>() else {
                return (CheckState::Unsupported, "auth_status_unavailable");
            };
            state
                .inner()
                .0
                .get_status()
                .await
                .accounts
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.reauth_required)
        }
        "github_copilot" => {
            let Some(state) = handle.try_state::<CopilotAuthState>() else {
                return (CheckState::Unsupported, "auth_status_unavailable");
            };
            let status = state.inner().0.read().await.get_status().await;
            status
                .accounts
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.reauth_required)
        }
        "xai_oauth" => {
            let Some(state) = handle.try_state::<XaiOAuthState>() else {
                return (CheckState::Unsupported, "auth_status_unavailable");
            };
            let status = state.inner().0.read().await.get_status().await;
            status
                .accounts
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.requires_reauth)
        }
        _ => None,
    };
    match needs_reauth {
        Some(false) => (CheckState::Configured, "oauth_bound_unverified"),
        Some(true) => (CheckState::Failed, "reauth_required"),
        None => (CheckState::Missing, "bound_account_missing"),
    }
}

async fn verify_api(
    db: Arc<Database>,
    handle: Option<tauri::AppHandle>,
    app: &AppType,
    provider: &Provider,
    model: Option<&str>,
    generate: bool,
) -> VerificationReport {
    let mut p = match target(app, provider) {
        Ok(p) => p,
        Err(reason) => {
            let mut r = VerificationReport::new(reason);
            r.authentication = CheckState::Unsupported;
            return r;
        }
    };
    // A diagnostic never sends configured prompt/body overrides or tools.
    if let Some(o) = p
        .meta
        .as_mut()
        .and_then(|m| m.local_proxy_request_overrides.as_mut())
    {
        o.body = None;
    }
    let managed = p
        .meta
        .as_ref()
        .and_then(|m| m.provider_type.as_deref())
        .is_some_and(|s| matches!(s, "codex_oauth" | "github_copilot" | "xai_oauth"));
    let has_key = ClaudeAdapter::new().extract_auth(&p).is_some();
    if !generate {
        let mut r = VerificationReport::new(if managed {
            "oauth_bound_unverified"
        } else if has_key {
            "credentials_configured"
        } else {
            "no_stored_credentials"
        });
        r.authentication = if has_key || managed {
            CheckState::Configured
        } else {
            CheckState::Missing
        };
        if managed {
            let (state, reason) = managed_login(handle.as_ref(), &p).await;
            r.authentication = state;
            r.reason = reason.into();
        }
        let result = super::stream_check::StreamCheckService::check_with_retry(
            &AppType::Claude,
            &p,
            &super::stream_check::StreamCheckConfig {
                timeout_secs: 5,
                max_retries: 0,
                degraded_threshold_ms: 6000,
            },
            None,
        )
        .await;
        r.endpoint = match result {
            Ok(v) => {
                r.http_status = v.http_status;
                if v.success {
                    CheckState::Reachable
                } else {
                    CheckState::Failed
                }
            }
            Err(_) => CheckState::Unsupported,
        };
        return r;
    }
    let model = model
        .filter(|m| !m.trim().is_empty())
        .map(str::to_owned)
        .or_else(|| model_for(provider));
    let Some(model) = model else {
        return VerificationReport::new("model_required");
    };
    if model.len() > 256 || model.chars().any(char::is_control) {
        return VerificationReport::new("model_required");
    }
    let request = json!({"model":model,"messages":[{"role":"user","content":PROMPT}],"max_tokens":64,"stream":false});
    let state = ProxyState::for_verification(db, handle);
    let operation = async {
        let ctx = RequestContext::new(
            &state,
            &request,
            &http::HeaderMap::new(),
            AppType::Claude,
            "Provider verification",
            "claude",
            Some(crate::mode::stack::StackTarget {
                provider: p.clone(),
                upstream_model: model.clone(),
                original_model: model,
            }),
        )
        .await?;
        let result = ctx
            .create_forwarder(&state)
            .forward_with_retry(
                &AppType::Claude,
                http::Method::POST,
                "/v1/messages",
                request,
                http::HeaderMap::new(),
                http::Extensions::new(),
                ctx.get_providers(),
            )
            .await
            .map_err(|e| e.error)?;
        let status = result.response.status().as_u16();
        let body = result.response.bytes_with_limit(MAX_BODY).await?;
        Ok::<_, ProxyError>(classify_response(status, &body))
    };
    match tokio::time::timeout(std::time::Duration::from_secs(60), operation).await {
        Ok(Ok(mut r)) => {
            if r.authentication == CheckState::Verified && !has_key && !managed {
                r.authentication = CheckState::NotRequired;
            }
            r
        }
        Ok(Err(error)) => failure(error),
        Err(_) => {
            let mut r = VerificationReport::new("timeout");
            r.model = CheckState::Failed;
            r
        }
    }
}

/// Run only the isolated profile command. Never inspect the default CLI account.
async fn verify_native(
    db: &Database,
    profile: &LaunchProfile,
    model: Option<&str>,
    generate: bool,
) -> VerificationReport {
    let mut command = match super::launch_profiles::command_by_id(db, &profile.id).await {
        Ok(command) => command,
        Err(_) => return VerificationReport::new("profile_unavailable"),
    };
    if !generate
        && profile.runtime == LaunchRuntime::Codex
        && profile.routing_mode != RoutingMode::Official
    {
        let provider = match super::launch_profiles::provider(db, profile) {
            Ok(p) => p,
            Err(_) => return VerificationReport::new("profile_unavailable"),
        };
        if let Ok(p) = target(&AppType::Codex, &provider) {
            let configured = ClaudeAdapter::new().extract_auth(&p).is_some();
            let mut report = VerificationReport::new(if configured {
                "credentials_configured"
            } else {
                "no_stored_credentials"
            });
            report.authentication = if configured {
                CheckState::Configured
            } else {
                CheckState::Missing
            };
            return report;
        }
    }
    if !generate && profile.runtime == LaunchRuntime::Opencode {
        // OpenCode auth list prints human output; inspect only the selected provider's private store.
        let file = profile.config_dir.join("data/opencode/auth.json");
        let stored = std::fs::metadata(&file)
            .ok()
            .filter(|m| m.len() <= MAX_BODY as u64)
            .and_then(|_| std::fs::read(&file).ok())
            .and_then(|v| serde_json::from_slice::<Value>(&v).ok());
        let credential = stored.as_ref().and_then(|v| v.get(&profile.provider_id));
        let expired = credential
            .and_then(|v| v.get("expires"))
            .and_then(Value::as_i64)
            .is_some_and(|expires| expires <= chrono::Utc::now().timestamp_millis());
        let inline = command
            .get_envs()
            .find(|(k, _)| *k == "OPENCODE_CONFIG_CONTENT")
            .and_then(|(_, v)| v)
            .and_then(|v| v.to_str())
            .and_then(|v| serde_json::from_str::<Value>(v).ok());
        let configured = inline
            .as_ref()
            .and_then(|v| v.get("provider").or_else(|| v.get("providers")))
            .and_then(|v| v.get(&profile.provider_id))
            .and_then(|v| str_at(v, &["/options/apiKey", "/settings/apiKey"]))
            .is_some();
        let mut r = VerificationReport::new(if expired {
            "credentials_expired"
        } else if credential.is_some() || configured {
            "credentials_configured"
        } else {
            "no_stored_credentials"
        });
        r.authentication = if expired {
            CheckState::Failed
        } else if credential.is_some() || configured {
            CheckState::Configured
        } else {
            CheckState::Missing
        };
        return r;
    }
    if generate {
        if let Some(model) = model.filter(|s| !s.trim().is_empty()) {
            if model.len() > 256 || model.chars().any(char::is_control) {
                return VerificationReport::new("model_required");
            }
            if profile.runtime == LaunchRuntime::Claude {
                command.env("ANTHROPIC_MODEL", model);
            }
        }
        match profile.runtime {
            LaunchRuntime::Claude => {
                command.args([
                    "--print",
                    "--output-format",
                    "json",
                    "--max-turns",
                    "1",
                    "--no-session-persistence",
                    "--tools",
                    "",
                    "--strict-mcp-config",
                    "--mcp-config",
                    r#"{"mcpServers":{}}"#,
                    PROMPT,
                ]);
            }
            LaunchRuntime::Codex => {
                command.args([
                    "exec",
                    "--json",
                    "--ephemeral",
                    "--skip-git-repo-check",
                    "--ignore-user-config",
                    "--ignore-rules",
                    "--sandbox",
                    "read-only",
                ]);
                if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
                    command.args(["--model", model]);
                }
                command.arg(PROMPT);
            }
            LaunchRuntime::Opencode => {
                command.args(["run", "--format", "json", "--pure"]);
                if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
                    let prefix = format!("{}/", profile.provider_id);
                    let model = if model.starts_with(&prefix) {
                        model.to_owned()
                    } else {
                        format!("{prefix}{model}")
                    };
                    command.args(["--model", &model]);
                }
                command.arg(PROMPT);
                command.env("OPENCODE_DISABLE_PROJECT_CONFIG", "true");
                let mut config = command
                    .get_envs()
                    .find(|(k, _)| *k == "OPENCODE_CONFIG_CONTENT")
                    .and_then(|(_, v)| v)
                    .and_then(|v| v.to_str())
                    .and_then(|v| serde_json::from_str::<Value>(v).ok())
                    .unwrap_or_else(|| json!({}));
                config["permission"] = json!({"*":"deny"});
                command.env("OPENCODE_CONFIG_CONTENT", config.to_string());
            }
        }
    } else {
        match profile.runtime {
            LaunchRuntime::Claude => {
                command.args(["auth", "status", "--json"]);
            }
            LaunchRuntime::Codex => {
                command.args(["login", "status"]);
            }
            LaunchRuntime::Opencode => unreachable!(),
        }
    }
    // Empty working directory prevents loading project instructions or reading project files.
    let directory = match tempfile::tempdir() {
        Ok(v) => v,
        Err(_) => return VerificationReport::new("request_failed"),
    };
    command.current_dir(directory.path());
    match run_native(command).await {
        Ok((success, stdout, _stderr)) if generate => {
            if !success {
                // Do not expose CLI diagnostics: providers may echo credentials in them.
                let mut r = VerificationReport::new("native_request_failed");
                r.model = CheckState::Failed;
                return r;
            }
            let mut report = classify_response(200, &stdout);
            // CLI output is not an observed HTTP response or proof of account identity.
            report.http_status = None;
            if profile.runtime == LaunchRuntime::Opencode && report.model == CheckState::Verified {
                report.authentication = CheckState::NotTested;
                report.reason = "native_model_verified".into();
            }
            report
        }
        Ok((success, stdout, stderr)) => {
            let logged_in = match profile.runtime {
                LaunchRuntime::Claude => {
                    success
                        && serde_json::from_slice::<Value>(&stdout)
                            .ok()
                            .and_then(|v| v.get("loggedIn").and_then(Value::as_bool))
                            == Some(true)
                }
                LaunchRuntime::Codex => {
                    success
                        && [&stdout, &stderr]
                            .iter()
                            .any(|b| String::from_utf8_lossy(b).contains("Logged in"))
                }
                LaunchRuntime::Opencode => false,
            };
            let mut r = VerificationReport::new(if logged_in {
                "native_signed_in_unverified"
            } else {
                "native_not_signed_in"
            });
            r.authentication = if logged_in {
                CheckState::SignedIn
            } else {
                CheckState::Missing
            };
            r
        }
        Err(reason) => {
            let mut r = VerificationReport::new(reason);
            if generate {
                r.model = CheckState::Failed;
            }
            r
        }
    }
}

async fn run_native(
    mut command: std::process::Command,
) -> Result<(bool, Vec<u8>, Vec<u8>), &'static str> {
    use tokio::io::AsyncReadExt;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| "cli_unavailable")?;
    let id = child.id();
    let stdout = child.stdout.take().ok_or("request_failed")?;
    let stderr = child.stderr.take().ok_or("request_failed")?;
    async fn read_bounded(
        reader: impl tokio::io::AsyncRead + Unpin,
    ) -> Result<Vec<u8>, &'static str> {
        let mut bytes = Vec::new();
        reader
            .take((MAX_BODY + 1) as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "request_failed")?;
        if bytes.len() > MAX_BODY {
            return Err("response_too_large");
        }
        Ok(bytes)
    }
    let operation = async {
        let (status, out, err) = tokio::try_join!(
            async { child.wait().await.map_err(|_| "request_failed") },
            read_bounded(stdout),
            read_bounded(stderr)
        )?;
        Ok((status.success(), out, err))
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), operation).await;
    // Include descendants which could keep an inherited pipe open after their parent exits.
    #[cfg(unix)]
    if let Some(id) = id {
        unsafe {
            libc::kill(-(id as libc::pid_t), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    if !matches!(&result, Ok(Ok(_))) {
        if let Some(id) = id {
            let _ = tokio::process::Command::new("taskkill")
                .args(["/PID", &id.to_string(), "/T", "/F"])
                .output()
                .await;
        }
    }
    match result {
        Ok(result) => {
            if result.is_err() {
                let _ = child.kill().await;
            }
            result
        }
        Err(_) => {
            let _ = child.kill().await;
            Err("timeout")
        }
    }
}

#[cfg(test)]
mod verification_tests {
    use super::*;
    fn provider(id: &str, config: Value) -> Provider {
        Provider::with_id(id.into(), id.into(), config, None)
    }

    #[test]
    fn http_authentication_failures_are_not_reachability_failures_or_success() {
        for (status, reason) in [
            (401, "unauthorized"),
            (403, "access_denied"),
            (429, "rate_limited"),
            (500, "upstream_error"),
        ] {
            let report = classify_response(status, br#"{"error":"mock-secret-do-not-expose"}"#);
            assert_eq!(report.endpoint, CheckState::Reachable);
            assert_ne!(report.authentication, CheckState::Verified);
            assert_ne!(report.model, CheckState::Verified);
            assert_eq!(report.reason, reason);
            assert!(!serde_json::to_string(&report)
                .unwrap()
                .contains("mock-secret"));
        }
    }
    #[test]
    fn supported_generated_text_formats_are_recognized_but_errors_win() {
        for value in [
            json!({"content":[{"type":"text","text":"OK"}]}),
            json!({"choices":[{"message":{"content":"OK"}}]}),
            json!({"output":[{"content":[{"text":"OK"}]}]}),
            json!({"candidates":[{"content":{"parts":[{"text":"OK"}]}}]}),
            json!({"result":"OK","is_error":false}),
            json!({"type":"text","part":{"text":"OK"}}),
            json!({"type":"item.completed","item":{"type":"agent_message","text":"OK"}}),
        ] {
            assert_eq!(
                classify_response(200, &serde_json::to_vec(&value).unwrap()).model,
                CheckState::Verified,
                "{value}"
            );
        }
        for body in ["data: {\"type\":\"response.output_text.delta\",\"delta\":\"OK\"}\n\ndata: {\"type\":\"response.failed\"}\n", "{\"result\":\"failed\",\"is_error\":true}","{\"models\":[\"test\"]}","<html>login</html>"] {
            assert_ne!(classify_response(200,body.as_bytes()).model,CheckState::Verified);
        }
    }
    #[test]
    fn native_auth_and_unbound_managed_accounts_never_use_a_global_default() {
        let mut p = provider("official", json!({}));
        p.category = Some("official".into());
        assert_eq!(
            target(&AppType::Codex, &p).unwrap_err(),
            "native_profile_required"
        );
        p.category = None;
        for kind in ["codex_oauth", "github_copilot", "xai_oauth"] {
            p.meta = serde_json::from_value(json!({"providerType":kind})).ok();
            assert_eq!(
                target(&AppType::Claude, &p).unwrap_err(),
                "account_binding_required"
            );
        }
        let p = provider(
            "implicit-copilot",
            json!({"env":{"ANTHROPIC_BASE_URL":"https://api.githubcopilot.com"}}),
        );
        assert_eq!(
            target(&AppType::Claude, &p).unwrap_err(),
            "account_binding_required"
        );
        let mut refresh_only = provider(
            "refresh-only",
            json!({"env":{"ANTHROPIC_BASE_URL":"https://generativelanguage.googleapis.com","ANTHROPIC_API_KEY":r#"{"refresh_token":"mock-refresh","client_secret":"mock-secret"}"#}}),
        );
        refresh_only.meta = Some(crate::provider::ProviderMeta {
            api_format: Some("gemini_native".into()),
            ..Default::default()
        });
        assert_eq!(
            target(&AppType::Claude, &refresh_only).unwrap_err(),
            "native_profile_required"
        );
    }
    #[test]
    fn persisted_schemas_use_existing_protocol_and_credential_fields() {
        let cases = [
            (
                AppType::Claude,
                json!({"env":{"ANTHROPIC_BASE_URL":"https://example.invalid","ANTHROPIC_AUTH_TOKEN":"mock-key"}}),
            ),
            (
                AppType::ClaudeDesktop,
                json!({"env":{"ANTHROPIC_BASE_URL":"https://example.invalid","ANTHROPIC_AUTH_TOKEN":"mock-key"}}),
            ),
            (
                AppType::Codex,
                json!({"auth":{"OPENAI_API_KEY":"mock-key"},"config":"model = \"test\"\nmodel_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://example.invalid/v1\"\nwire_api = \"responses\""}),
            ),
            (
                AppType::GrokBuild,
                json!({"auth":{"OPENAI_API_KEY":"mock-key"},"config":"model_provider = \"custom\"\n[model_providers.custom]\nbase_url = \"https://example.invalid/v1\"\nwire_api = \"responses\""}),
            ),
            (
                AppType::Gemini,
                json!({"env":{"GOOGLE_GEMINI_BASE_URL":"https://example.invalid","GEMINI_API_KEY":"mock-key"}}),
            ),
            (
                AppType::Agy,
                json!({"env":{"GOOGLE_GEMINI_BASE_URL":"https://example.invalid","GEMINI_API_KEY":"mock-key"}}),
            ),
            (
                AppType::OpenCode,
                json!({"npm":"@ai-sdk/openai-compatible","options":{"baseURL":"https://example.invalid/v1","apiKey":"mock-key"}}),
            ),
            (
                AppType::OpenClaw,
                json!({"baseUrl":"https://example.invalid/v1","apiKey":"mock-key","api":"openai-completions"}),
            ),
            (
                AppType::Hermes,
                json!({"base_url":"https://example.invalid/v1","api_key":"mock-key","api_mode":"chat_completions"}),
            ),
            (
                AppType::Pi,
                json!({"baseUrl":"https://example.invalid/v1","apiKey":"mock-key","api":"openai-completions"}),
            ),
            (
                AppType::Omp,
                json!({"baseUrl":"https://example.invalid/v1","apiKey":"mock-key","api":"openai-completions"}),
            ),
            (
                AppType::Mcode,
                json!({"api":"openai","options":{"baseURL":"https://example.invalid/v1","apiKey":"mock-key"}}),
            ),
            (
                AppType::Dsh,
                json!({"api":"openai","options":{"baseURL":"https://example.invalid/v1","apiKey":"mock-key"}}),
            ),
            (
                AppType::Kimi,
                json!({"provider":{"base_url":"https://example.invalid/v1","api_key":"mock-key","type":"openai"}}),
            ),
        ];
        for (app, config) in cases {
            let normalized =
                target(&app, &provider("test", config)).unwrap_or_else(|e| panic!("{app:?}: {e}"));
            let auth = ClaudeAdapter::new()
                .extract_auth(&normalized)
                .unwrap_or_else(|| panic!("{app:?}: key missing"));
            assert_eq!(auth.api_key, "mock-key", "{app:?}");
        }
    }
    #[tokio::test]
    async fn selected_provider_is_used_without_failover_and_inspection_does_not_generate() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        use axum::{
            routing::{get, post},
            Json, Router,
        };
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let app=Router::new().route("/",get(||async{axum::http::StatusCode::UNAUTHORIZED})).route("/v1/messages",post(move |headers:axum::http::HeaderMap,Json(body):Json<Value>|{let count=count.clone();async move{
            count.fetch_add(1,Ordering::SeqCst);
            assert_eq!(headers.get("authorization").unwrap(),"Bearer mock-selected-key");
            assert_eq!(body["model"],"mock-model");assert_eq!(body["messages"][0]["content"],PROMPT);
            assert!(body.get("tools").is_none());
            Json(json!({"content":[{"type":"text","text":"CC_SWITCH_CONNECTION_OK"}]}))
        }}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let db = Arc::new(Database::memory().unwrap());
        let selected = provider(
            "selected",
            json!({"env":{"ANTHROPIC_BASE_URL":format!("http://{addr}"),"ANTHROPIC_AUTH_TOKEN":"mock-selected-key","ANTHROPIC_MODEL":"mock-model"}}),
        );
        let other = provider(
            "global",
            json!({"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:1","ANTHROPIC_AUTH_TOKEN":"mock-other-key"}}),
        );
        db.save_provider("claude", &selected).unwrap();
        db.save_provider("claude", &other).unwrap();
        db.set_current_provider("claude", "global").unwrap();
        let r = verify_provider(
            db.clone(),
            None,
            AppType::Claude,
            "selected",
            None,
            None,
            false,
        )
        .await
        .unwrap();
        assert_eq!(r.endpoint, CheckState::Reachable);
        assert_eq!(r.http_status, Some(401));
        assert_eq!(r.authentication, CheckState::Configured);
        assert_eq!(r.model, CheckState::NotTested);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let r = verify_provider(
            db.clone(),
            None,
            AppType::Claude,
            "selected",
            None,
            None,
            true,
        )
        .await
        .unwrap();
        assert_eq!(r.model, CheckState::Verified, "{}", r.reason);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            db.get_current_provider("claude").unwrap().as_deref(),
            Some("global")
        );
        assert!(!serde_json::to_string(&r)
            .unwrap()
            .contains("mock-selected-key"));
        assert!(
            verify_provider(db, None, AppType::Claude, "missing", None, None, true)
                .await
                .is_err()
        );
        task.abort();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn native_execution_preserves_literal_arguments_exit_and_bounded_output() {
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args([
            "-c",
            "printf '%s' \"$1\"; exit 37",
            "test",
            "$(must-not-run)",
        ]);
        let (success, out, _) = run_native(cmd).await.unwrap();
        assert!(!success);
        assert_eq!(out, b"$(must-not-run)");
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", "printf '{\"loggedIn\":true}'"]);
        let (_, out, _) = run_native(cmd).await.unwrap();
        assert_ne!(classify_response(200, &out).model, CheckState::Verified);
        let mut cmd = std::process::Command::new("/usr/bin/head");
        cmd.args(["-c", &(MAX_BODY + 2).to_string(), "/dev/zero"]);
        assert_eq!(run_native(cmd).await.unwrap_err(), "response_too_large");
    }
    #[tokio::test]
    async fn a_profile_cannot_inspect_another_provider_or_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::memory().unwrap());
        for id in ["one", "other"] {
            let mut p = provider(id, json!({}));
            p.category = Some("official".into());
            db.save_provider("claude", &p).unwrap();
            db.save_provider("codex", &p).unwrap();
        }
        let profile = super::super::launch_profiles::save(
            &db,
            LaunchProfile {
                id: "private-one".into(),
                name: "One".into(),
                launcher: "private-one".into(),
                provider_id: "one".into(),
                runtime: LaunchRuntime::Claude,
                config_dir: directory.path().canonicalize().unwrap().join("private-one"),
                routing_mode: RoutingMode::Official,
                model: None,
                revision: 0,
                created_at: 0,
                updated_at: 0,
                is_default: false,
            },
        )
        .unwrap();
        for (app, id) in [(AppType::Claude, "other"), (AppType::Codex, "one")] {
            let result =
                verify_provider(db.clone(), None, app, id, Some(&profile.id), None, false).await;
            assert!(result.unwrap_err().contains("does not belong"));
        }
        db.delete_launch_profile(&profile.id).unwrap();
        assert!(verify_provider(
            db,
            None,
            AppType::Claude,
            "one",
            Some(&profile.id),
            None,
            false
        )
        .await
        .unwrap_err()
        .contains("no longer exists"));
    }
}
