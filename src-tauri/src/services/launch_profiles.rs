//! Isolated Claude launches. Deliberately separate from project snapshot profiles.
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{database::Database, error::AppError, provider::Provider};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchProfile {
    pub id: String,
    pub name: String,
    pub launcher: String,
    pub provider_id: String,
    #[serde(default)]
    pub runtime: LaunchRuntime,
    #[serde(default)]
    pub is_default: bool,
    pub config_dir: PathBuf,
    pub routing_mode: RoutingMode,
    pub model: Option<String>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchRuntime {
    #[default]
    Claude,
    Codex,
    Opencode,
}
impl LaunchRuntime {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    Official,
    Proxy,
    Direct,
}

fn invalid(message: &str) -> AppError {
    AppError::InvalidInput(message.into())
}

pub(crate) fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

pub(crate) fn fingerprint(provider: &Provider) -> Result<String, AppError> {
    let mut meta = provider.meta.clone().unwrap_or_default();
    // Endpoints are stored separately by the existing DAO.
    meta.custom_endpoints.clear();
    let value = serde_json::json!([provider.settings_config, meta, provider.category]);
    Ok(digest(
        crate::proxy::json_canonical::canonical_json_string(&value).as_bytes(),
    ))
}

pub fn provider(db: &Database, profile: &LaunchProfile) -> Result<Provider, AppError> {
    let provider = db
        .get_provider_by_id(&profile.provider_id, profile.runtime.as_str())?
        .ok_or_else(|| invalid("Profile provider is missing; select a valid Claude provider"))?;
    if profile.runtime != LaunchRuntime::Claude {
        if profile.routing_mode == RoutingMode::Proxy {
            return Err(invalid("This runtime requires a direct profile; Claude proxy routing is not interchangeable"));
        }
        if profile.runtime == LaunchRuntime::Codex
            && crate::tray::provider_needs_routing(&crate::app_config::AppType::Codex, &provider)
        {
            return Err(invalid("This Codex provider requires routing and cannot yet be launched as an isolated native Codex profile"));
        }
        return Ok(provider);
    }
    let official = provider.category.as_deref() == Some("official");
    if official != (profile.routing_mode == RoutingMode::Official) {
        return Err(invalid(
            "Official providers require official mode; other providers require proxy mode",
        ));
    }
    // OAuth must never follow a mutable default account.
    if let Some(meta) = &provider.meta {
        if let Some(kind) = meta.provider_type.as_deref() {
            if matches!(kind, "codex_oauth" | "xai_oauth" | "github_copilot")
                && meta
                    .managed_account_id_for(kind)
                    .filter(|id| !id.trim().is_empty())
                    .is_none()
            {
                return Err(invalid(
                    "Profile OAuth provider requires an explicit account binding",
                ));
            }
        }
    }
    Ok(provider)
}

// Resolve existing ancestors so case aliases on macOS cannot bypass overlap checks.
fn normalized_directory(path: &Path) -> Result<PathBuf, AppError> {
    let ancestor = path
        .ancestors()
        .find(|p| p.exists())
        .ok_or_else(|| invalid("Configuration directory has no accessible ancestor"))?;
    let base = ancestor
        .canonicalize()
        .map_err(|e| AppError::io(ancestor, e))?;
    Ok(base.join(
        path.strip_prefix(ancestor)
            .map_err(|_| invalid("Invalid configuration path"))?,
    ))
}

pub fn validate(profile: &LaunchProfile, default_dir: &Path) -> Result<(), AppError> {
    if profile.name.trim().is_empty() || profile.name.len() > 128 {
        return Err(invalid("Profile name must contain 1–128 bytes"));
    }
    if matches!(
        profile.launcher.as_str(),
        "cc-switch" | "profile" | "cd" | "exec" | "eval" | "exit" | "source"
    ) {
        return Err(invalid(
            "Choose a launcher name that does not replace an existing CLI or shell command",
        ));
    }
    if profile.launcher.is_empty()
        || profile.launcher.len() > 64
        || !profile
            .launcher
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        || !profile.launcher.as_bytes()[0].is_ascii_alphabetic()
    {
        return Err(invalid(
            "Launcher must begin with a letter and contain only ASCII letters, digits, - or _",
        ));
    }
    if profile.id.is_empty()
        || profile.id.len() > 64
        || !profile
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(invalid("Invalid profile ID"));
    }
    let path = &profile.config_dir;
    if !path.is_absolute()
        || path.parent().is_none()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid(
            "Choose an absolute profile directory outside default Claude storage",
        ));
    }
    for ancestor in path.ancestors() {
        if std::fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(invalid("Profile directory must not contain symlinks"));
        }
    }
    let normalized = normalized_directory(path)?;
    let default = normalized_directory(default_dir)?;
    if normalized.starts_with(&default) || default.starts_with(&normalized) {
        return Err(invalid("Profile directory overlaps default Claude storage"));
    }
    if path.exists() && !path.is_dir() {
        return Err(invalid("Profile configuration path is not a directory"));
    }
    Ok(())
}

pub fn save(db: &Database, mut profile: LaunchProfile) -> Result<LaunchProfile, AppError> {
    if profile.id.is_empty() {
        profile.id = uuid::Uuid::new_v4().to_string();
    }
    if profile.config_dir.as_os_str().is_empty() {
        profile.config_dir = crate::config::get_app_config_dir()
            .join("profiles")
            .join(&profile.id);
    }
    validate(&profile, &crate::config::get_claude_config_dir())?;
    validate(&profile, &crate::config::get_home_dir().join(".claude"))?;
    for dir in [
        crate::codex_config::get_codex_config_dir(),
        crate::config::get_home_dir().join(".codex"),
        crate::opencode_config::get_opencode_dir(),
        crate::config::get_home_dir().join(".config/opencode"),
    ] {
        validate(&profile, &dir)?;
    }
    profile.config_dir = normalized_directory(&profile.config_dir)?;
    let app_dir = normalized_directory(&crate::config::get_app_config_dir())?;
    let profiles_dir = app_dir.join("profiles");
    if profile.config_dir.starts_with(&app_dir)
        && (!profile.config_dir.starts_with(&profiles_dir) || profile.config_dir == profiles_dir)
    {
        return Err(invalid(
            "Choose a directory outside CC Switch data, or inside its profiles directory",
        ));
    }
    provider(db, &profile)?;
    db.save_launch_profile(profile)
}

pub fn prepare_directory(path: &Path) -> Result<(), AppError> {
    // Never write settings, login, MCP, Skills or other existing files here.
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|e| AppError::io(path, e))
}

/// Refuse pre-existing routing overrides rather than overwriting user settings.
fn check_settings(path: &Path) -> Result<(), AppError> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AppError::io(path, error)),
    };
    let document = crate::jsonc_document::JsoncDocument::parse(&source).map_err(|_| {
        invalid("Cannot validate profile/project settings; correct their JSON first")
    })?;
    let settings = document.value();
    let has_override = settings.get("apiKeyHelper").is_some_and(|v| !v.is_null())
        || settings
            .get("env")
            .and_then(|v| v.as_object())
            .is_some_and(|env| {
                env.keys().any(|key| {
                    matches!(
                        key.as_str(),
                        "ANTHROPIC_BASE_URL"
                            | "ANTHROPIC_AUTH_TOKEN"
                            | "ANTHROPIC_API_KEY"
                            | "CLAUDE_CONFIG_DIR"
                            | "CLAUDE_SECURESTORAGE_CONFIG_DIR"
                            | "CLAUDE_CODE_OAUTH_TOKEN"
                    ) || key.starts_with("CLAUDE_CODE_USE_")
                })
            });
    if has_override {
        return Err(AppError::InvalidInput(format!("Routing/authentication override in {} conflicts with an isolated profile; remove it explicitly before launching", path.display())));
    }
    Ok(())
}

pub async fn command(db: &Database, alias: &str) -> Result<std::process::Command, AppError> {
    let profile = db
        .list_launch_profiles()?
        .into_iter()
        .find(|p| p.launcher == alias)
        .ok_or_else(|| invalid("Unknown launch profile"))?;
    profile_command(db, profile).await
}

pub(crate) async fn command_by_id(
    db: &Database,
    id: &str,
) -> Result<std::process::Command, AppError> {
    let profile = db
        .list_launch_profiles()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| invalid("This launcher profile was removed; install a new launcher"))?;
    profile_command(db, profile).await
}

/// Forward model configuration without copying provider credentials into the child.
/// The profile's explicit startup model takes precedence over the provider default.
fn apply_claude_model_env(
    command: &mut std::process::Command,
    settings: &serde_json::Value,
    startup_model: Option<&str>,
) {
    let env = settings.get("env").and_then(serde_json::Value::as_object);
    if let Some(env) = env {
        for key in ["ANTHROPIC_MODEL", "CLAUDE_CODE_SUBAGENT_MODEL"] {
            if let Some(value) = env.get(key).and_then(serde_json::Value::as_str) {
                command.env(key, value);
            }
        }
        for base in [
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
            "ANTHROPIC_CUSTOM_MODEL_OPTION",
        ] {
            for suffix in ["", "_NAME", "_DESCRIPTION", "_SUPPORTED_CAPABILITIES"] {
                let key = format!("{base}{suffix}");
                if let Some(value) = env.get(&key).and_then(serde_json::Value::as_str) {
                    command.env(key, value);
                }
            }
        }
    }
    if let Some(model) = startup_model {
        command.env("ANTHROPIC_MODEL", model);
    }
}

async fn profile_command(
    db: &Database,
    profile: LaunchProfile,
) -> Result<std::process::Command, AppError> {
    validate(&profile, &crate::config::get_claude_config_dir())?;
    let selected = provider(db, &profile)?;
    if profile.runtime != LaunchRuntime::Claude {
        return native_command(&profile, &selected);
    }
    check_settings(&profile.config_dir.join("settings.json"))?;
    let cwd =
        std::env::current_dir().map_err(|_| invalid("Cannot resolve launch working directory"))?;
    for dir in cwd.ancestors() {
        // The home-level .claude is user storage, not shared project settings.
        if dir == crate::config::get_home_dir() || dirs::home_dir().as_deref() == Some(dir) {
            continue;
        }
        for name in ["settings.json", "settings.local.json"] {
            check_settings(&dir.join(".claude").join(name))?;
        }
        if dir.join(".git").exists() {
            break;
        }
    }
    prepare_directory(&profile.config_dir)?;
    let mut command = std::process::Command::new("claude");
    crate::services::claude_account::login::clear_account_env(&mut command);
    command
        .env("CLAUDE_CONFIG_DIR", &profile.config_dir)
        .env("CLAUDE_SECURESTORAGE_CONFIG_DIR", &profile.config_dir);
    apply_claude_model_env(
        &mut command,
        &selected.settings_config,
        profile.model.as_deref(),
    );
    if profile.routing_mode == RoutingMode::Proxy {
        let config = db.get_proxy_config().await?;
        if !matches!(
            config.listen_address.as_str(),
            "127.0.0.1" | "localhost" | "::1"
        ) {
            return Err(invalid("Profile routing requires a loopback proxy address"));
        }
        let host = if config.listen_address == "::1" {
            "[::1]"
        } else {
            &config.listen_address
        };
        let base = format!("http://{host}:{}", config.listen_port);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .map_err(|_| invalid("Cannot initialize proxy check"))?;
        client
            .get(format!("{base}/health"))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|_| invalid("CC Switch proxy is unavailable; start local routing first"))?;
        let token = format!(
            "ccsp_{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        db.issue_launch_session(
            &digest(token.as_bytes()),
            &profile,
            &fingerprint(&selected)?,
        )?;
        command
            .env("ANTHROPIC_BASE_URL", format!("{base}/profiles"))
            .env("ANTHROPIC_AUTH_TOKEN", token)
            .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
    }
    Ok(command)
}

/// Names are explicit selectors; an unknown name never falls back to another account.
pub(crate) fn dispatch_target(
    db: &Database,
    args: &[std::ffi::OsString],
) -> Result<(String, Vec<std::ffi::OsString>), AppError> {
    let profiles = db.list_launch_profiles()?;
    if let Some(first) = args
        .first()
        .filter(|v| !v.to_string_lossy().starts_with('-') && !v.to_string_lossy().starts_with('/'))
    {
        let profile = profiles
            .into_iter()
            .find(|p| first == p.launcher.as_str())
            .ok_or_else(|| invalid("Unknown profile name; choose an existing launch profile"))?;
        return Ok((profile.id, args[1..].to_vec()));
    }
    let mut defaults = profiles.into_iter().filter(|p| p.is_default);
    let profile = defaults.next().ok_or_else(|| {
        invalid("No default profile selected. Choose a default in Launch profiles.")
    })?;
    if defaults.next().is_some() {
        return Err(invalid(
            "Multiple defaults found; choose a default again in Launch profiles.",
        ));
    }
    let forwarded = if args.first().is_some_and(|a| a == "--") {
        &args[1..]
    } else {
        args
    };
    Ok((profile.id, forwarded.to_vec()))
}

/// Invoked before GUI/single-instance initialization. No secrets enter argv.
pub fn cli(args: Vec<std::ffi::OsString>) -> i32 {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let result = (|| -> Result<i32, AppError> {
        let db = Database::init()?;
        let args = if args.first().is_some_and(|a| a == "dispatch") {
            let (id, forwarded) = dispatch_target(&db, &args[1..])?;
            let mut resolved = vec![std::ffi::OsString::from("run-id"), id.into()];
            resolved.extend(forwarded);
            resolved
        } else {
            args
        };
        if args.len() < 2 || (args[0] != "run" && args[0] != "run-id") {
            return Err(invalid(
                "Usage: cc-switch profile run <launcher> [Claude arguments...]",
            ));
        }
        let alias = args[1]
            .to_str()
            .ok_or_else(|| invalid("Invalid launcher"))?;
        let runtime =
            tokio::runtime::Runtime::new().map_err(|_| invalid("Cannot start launcher runtime"))?;
        let mut cmd = if args[0] == "run-id" {
            runtime.block_on(command_by_id(&db, alias))?
        } else {
            runtime.block_on(command(&db, alias))?
        };
        cmd.args(&args[2..]);
        drop(runtime);
        drop(db);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let error = cmd.exec();
            Err(AppError::Message(format!("Cannot launch claude: {error}")))
        }
        #[cfg(not(unix))]
        {
            cmd.status()
                .map(|s| s.code().unwrap_or(1))
                .map_err(|e| AppError::Message(format!("Cannot launch claude: {e}")))
        }
    })();
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile(path: PathBuf) -> LaunchProfile {
        LaunchProfile {
            id: "test".into(),
            name: "Test".into(),
            launcher: "claude-test".into(),
            provider_id: "mock".into(),
            runtime: crate::services::launch_profiles::LaunchRuntime::Claude,
            config_dir: path,
            routing_mode: RoutingMode::Official,
            model: None,
            revision: 0,
            created_at: 0,
            is_default: false,
            updated_at: 0,
        }
    }
    #[test]
    fn launcher_passes_model_picker_names_without_provider_credentials() {
        let settings = serde_json::json!({"env": {
            "ANTHROPIC_MODEL": "deepseek-v4-flash",
            "ANTHROPIC_DEFAULT_SONNET_MODEL": "deepseek-v4-flash",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "DeepSeek Flash · OpenCode Go",
            "ANTHROPIC_DEFAULT_OPUS_MODEL": "custom-pro",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME": "My Pro",
            "ANTHROPIC_CUSTOM_MODEL_OPTION": "custom-extra",
            "ANTHROPIC_CUSTOM_MODEL_OPTION_NAME": "My Extra",
            "ANTHROPIC_API_KEY": "must-not-leak",
            "ANTHROPIC_BASE_URL": "https://must-not-bypass.invalid"
        }});
        let mut command = std::process::Command::new("claude");
        apply_claude_model_env(&mut command, &settings, None);
        let env: std::collections::HashMap<_, _> = command.get_envs().collect();
        for key in [
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "ANTHROPIC_CUSTOM_MODEL_OPTION",
            "ANTHROPIC_CUSTOM_MODEL_OPTION_NAME",
        ] {
            assert_eq!(
                env[std::ffi::OsStr::new(key)].unwrap(),
                settings["env"][key].as_str().unwrap()
            );
        }
        assert!(!env.contains_key(std::ffi::OsStr::new("ANTHROPIC_API_KEY")));
        assert!(!env.contains_key(std::ffi::OsStr::new("ANTHROPIC_BASE_URL")));
    }

    #[test]
    fn launcher_startup_override_keeps_the_provider_model_menu() {
        let mut command = std::process::Command::new("claude");
        apply_claude_model_env(
            &mut command,
            &serde_json::json!({"env": {
                "ANTHROPIC_MODEL": "provider-default",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "provider-sonnet",
                "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "My Sonnet"
            }}),
            Some("profile-startup"),
        );
        let env: std::collections::HashMap<_, _> = command.get_envs().collect();
        assert_eq!(
            env[std::ffi::OsStr::new("ANTHROPIC_MODEL")].unwrap(),
            "profile-startup"
        );
        assert_eq!(
            env[std::ffi::OsStr::new("ANTHROPIC_DEFAULT_SONNET_MODEL_NAME")].unwrap(),
            "My Sonnet"
        );
    }

    #[test]
    fn launch_profile_rejects_shell_injection_and_default_directory() {
        let default = Path::new("/Users/launch-profile-test-user/.claude");
        let mut p = profile("/Users/launch-profile-test-user/profiles/test".into());
        assert!(validate(&p, default).is_ok());
        for alias in ["a;echo", "$(id)", "a b", "-flag", "a\nexit"] {
            p.launcher = alias.into();
            assert!(validate(&p, default).is_err());
        }
        p.launcher = "claude-test".into();
        for path in [
            "/Users/launch-profile-test-user/.claude",
            "/Users/launch-profile-test-user/.claude/sub",
            "/Users/launch-profile-test-user",
            "relative",
            "/Users/launch-profile-test-user/profiles/../.claude",
        ] {
            p.config_dir = path.into();
            assert!(validate(&p, default).is_err());
        }
    }
    #[test]
    fn launch_profile_rejects_imported_routing_overrides_without_modifying_them() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("settings.json");
        let source = br#"{"env":{"ANTHROPIC_BASE_URL":"http://wrong.invalid"},"permissions":{}}"#;
        std::fs::write(&file, source).unwrap();
        assert!(check_settings(&file).is_err());
        assert_eq!(std::fs::read(&file).unwrap(), source);
        std::fs::write(
            &file,
            br#"{"env":{"ANTHROPIC_MODEL":"optional-model"},"permissions":{}}"#,
        )
        .unwrap();
        assert!(check_settings(&file).is_ok());
    }
    #[test]
    fn launch_profile_directory_creation_preserves_existing_user_files() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("profile");
        prepare_directory(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(&file, b"{\"custom\":true}").unwrap();
        prepare_directory(&dir).unwrap();
        assert_eq!(std::fs::read(file).unwrap(), b"{\"custom\":true}");
        assert!(!dir.join(".credentials.json").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
    }
}

#[cfg(all(test, unix))]
mod launcher_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[tokio::test]
    async fn launch_profiles_forward_arguments_exit_codes_and_isolate_concurrent_children() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let bin = root.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let fake = bin.join("claude");
        std::fs::write(&fake, "#!/bin/sh\nprintf '%s\\n' \"$CLAUDE_CONFIG_DIR\" \"$CLAUDE_SECURESTORAGE_CONFIG_DIR\" \"$PWD\" \"$@\"\nexit 37\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        let db = Database::memory().unwrap();
        let mut official = Provider::with_id(
            "official".into(),
            "Official".into(),
            serde_json::json!({}),
            None,
        );
        official.category = Some("official".into());
        db.save_provider("claude", &official).unwrap();
        for name in ["a", "b"] {
            save(
                &db,
                LaunchProfile {
                    id: name.into(),
                    name: name.into(),
                    launcher: format!("claude-{name}"),
                    provider_id: "official".into(),
                    runtime: crate::services::launch_profiles::LaunchRuntime::Claude,
                    config_dir: root.join(name),
                    routing_mode: RoutingMode::Official,
                    model: None,
                    revision: 0,
                    created_at: 0,
                    is_default: false,
                    updated_at: 0,
                },
            )
            .unwrap();
        }
        let mut a = command(&db, "claude-a").await.unwrap();
        let mut b = command(&db, "claude-b").await.unwrap();
        for cmd in [&mut a, &mut b] {
            cmd.env("PATH", &bin).current_dir(&root).args([
                "space argument",
                "$(echo must-not-run)",
                "--resume",
                "session",
            ]);
            assert!(cmd
                .get_envs()
                .any(|(k, v)| k == "CLAUDE_CONFIG_DIR" && v.is_some()));
        }
        let ta = std::thread::spawn(move || a.output().unwrap());
        let tb = std::thread::spawn(move || b.output().unwrap());
        for (name, output) in [("a", ta.join().unwrap()), ("b", tb.join().unwrap())] {
            assert_eq!(output.status.code(), Some(37));
            let expected = format!("{0}/{name}\n{0}/{name}\n{0}\nspace argument\n$(echo must-not-run)\n--resume\nsession\n", root.display());
            assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        }
        assert!(command(&db, "missing").await.is_err());
    }
}

fn native_command(
    profile: &LaunchProfile,
    selected: &Provider,
) -> Result<std::process::Command, AppError> {
    prepare_directory(&profile.config_dir)?;
    let mut cmd = std::process::Command::new(profile.runtime.as_str());
    crate::services::claude_account::login::clear_account_env(&mut cmd);
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy();
        if name.starts_with("OPENCODE_")
            || name.starts_with("CODEX_")
            || matches!(
                name.as_ref(),
                "OPENAI_API_KEY"
                    | "OPENAI_BASE_URL"
                    | "GEMINI_API_KEY"
                    | "GOOGLE_API_KEY"
                    | "GROQ_API_KEY"
                    | "OPENROUTER_API_KEY"
            )
        {
            cmd.env_remove(key);
        }
    }
    match profile.runtime {
        LaunchRuntime::Codex => {
            cmd.env("CODEX_HOME", &profile.config_dir);
            let path = profile.config_dir.join("config.toml");
            let local = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => return Err(AppError::io(&path, error)),
            };
            // Reuse the existing leftover-field checks against this profile, never global state.
            if selected.category.as_deref() != Some("official") {
                let plan = crate::services::provider::codex_terminal::build(selected, &local)?;
                for value in plan.overrides {
                    cmd.arg("-c").arg(value);
                }
                for (key, value) in plan.env {
                    if let Some(value) = value {
                        cmd.env(key, value);
                    } else {
                        cmd.env_remove(key);
                    }
                }
                if let Some(catalog) = plan.catalog {
                    let file = profile
                        .config_dir
                        .join(format!("catalog-{}.json", uuid::Uuid::new_v4()));
                    crate::config::atomic_write_private(&file, &catalog)?;
                    cmd.arg("-c")
                        .arg(crate::services::provider::codex_terminal::catalog_override(
                            &file.to_string_lossy(),
                        ));
                }
            }
            if selected.category.as_deref() == Some("official") {
                let doc: toml::Value =
                    toml::from_str(&local).map_err(|_| invalid("Invalid profile Codex config"))?;
                if doc.get("profile").is_some()
                    || doc
                        .get("model_provider")
                        .and_then(|v| v.as_str())
                        .is_some_and(|v| v != "openai")
                {
                    return Err(invalid("This profile's Codex config overrides official routing; remove that override explicitly"));
                }
            }
            if let Some(model) = &profile.model {
                cmd.arg("--model").arg(model);
            }
        }
        LaunchRuntime::Opencode => {
            let (fragment, format) = crate::opencode_config::provider_fragment(
                &selected.id,
                &selected.settings_config,
                selected.opencode_config_format(),
            )?;
            let model = profile
                .model
                .clone()
                .or_else(|| {
                    fragment
                        .get("models")
                        .and_then(|v| v.as_object())
                        .and_then(|v| v.keys().next().cloned())
                })
                .filter(|m| !m.trim().is_empty())
                .ok_or_else(|| invalid("Choose an OpenCode model for this launcher"))?;
            let model = if model.starts_with(&format!("{}/", selected.id)) {
                model
            } else {
                format!("{}/{model}", selected.id)
            };
            let key = match format {
                crate::provider::OpenCodeConfigFormat::V1 => "provider",
                crate::provider::OpenCodeConfigFormat::V2 => "providers",
            };
            let mut config = serde_json::json!({"model":model, "enabled_providers":[selected.id]});
            config[key] = serde_json::json!({selected.id.clone():fragment});
            cmd.env(
                "OPENCODE_CONFIG_CONTENT",
                serde_json::to_string(&config).map_err(|_| invalid("Invalid OpenCode profile"))?,
            );
            cmd.env("OPENCODE_CONFIG_DIR", &profile.config_dir)
                .env("XDG_DATA_HOME", profile.config_dir.join("data"))
                .env("XDG_CONFIG_HOME", profile.config_dir.join("config"))
                .env("XDG_CACHE_HOME", profile.config_dir.join("cache"))
                .env("XDG_STATE_HOME", profile.config_dir.join("state"))
                .arg("--model")
                .arg(model);
        }
        LaunchRuntime::Claude => unreachable!(),
    }
    Ok(cmd)
}

#[cfg(test)]
mod native_runtime_tests {
    use super::*;
    #[tokio::test]
    async fn profiles_resolve_identical_provider_ids_in_their_runtime_namespace() {
        let db = Database::memory().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        for runtime in [LaunchRuntime::Codex, LaunchRuntime::Opencode] {
            let mut p = Provider::with_id(
                "shared".into(),
                runtime.as_str().into(),
                serde_json::json!({"models":{"model-one":{}}}),
                None,
            );
            p.category = Some("official".into());
            db.save_provider(runtime.as_str(), &p).unwrap();
            let profile:LaunchProfile=serde_json::from_value(serde_json::json!({"id":runtime.as_str(),"name":runtime.as_str(),"launcher":format!("{}-isolated",runtime.as_str()),"providerId":"shared","runtime":runtime,"configDir":root.join(runtime.as_str()),"routingMode":if runtime==LaunchRuntime::Codex{"official"}else{"direct"},"model":null})).unwrap();
            let saved = save(&db, profile).unwrap();
            assert_eq!(provider(&db, &saved).unwrap().name, runtime.as_str());
            let command = command(&db, &saved.launcher).await.unwrap();
            assert_eq!(command.get_program(), runtime.as_str());
            let env: std::collections::HashMap<_, _> = command
                .get_envs()
                .filter_map(|(k, v)| {
                    v.map(|v| {
                        (
                            k.to_string_lossy().to_string(),
                            v.to_string_lossy().to_string(),
                        )
                    })
                })
                .collect();
            if runtime == LaunchRuntime::Codex {
                assert_eq!(env["CODEX_HOME"], root.join("codex").to_string_lossy());
            } else {
                let config: serde_json::Value =
                    serde_json::from_str(&env["OPENCODE_CONFIG_CONTENT"]).unwrap();
                assert_eq!(config["enabled_providers"], serde_json::json!(["shared"]));
                assert_eq!(config["model"], "shared/model-one");
                assert!(env["XDG_DATA_HOME"]
                    .starts_with(&root.join("opencode").to_string_lossy().to_string()));
            }
        }
    }
    #[test]
    fn legacy_profile_payloads_still_launch_claude() {
        let p:LaunchProfile=serde_json::from_value(serde_json::json!({"id":"legacy","name":"Legacy","launcher":"legacy-claude","providerId":"a","configDir":"/private/legacy-profile","routingMode":"official","model":null})).unwrap();
        assert_eq!(p.runtime, LaunchRuntime::Claude);
    }
}
