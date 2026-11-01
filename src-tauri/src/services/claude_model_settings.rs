//! Context configuration for custom Claude Code startup model IDs.
use serde_json::{Map, Value};

pub(crate) const CONTEXT_WINDOW: &str = "CLAUDE_CODE_MAX_CONTEXT_TOKENS";

fn custom_model(model: &str) -> bool {
    let lower = model.trim().to_ascii_lowercase();
    !lower.is_empty()
        && !matches!(
            lower.as_str(),
            "default" | "best" | "sonnet" | "opus" | "haiku" | "fable" | "opusplan"
        )
        && !lower.contains("[1m]")
        && !lower.contains("claude-")
        && !lower.starts_with("arn:")
}

/// Unknown IDs already use Claude Code's 200K fallback. Declare that same limit
/// to avoid the unknown-context warning, while preserving a provider's explicit
/// (possibly smaller) window. This does not claim to discover the upstream limit.
pub(crate) fn ensure_context_window(env: &mut Map<String, Value>, model: Option<&str>) {
    let selected = model.or_else(|| env.get("ANTHROPIC_MODEL").and_then(Value::as_str));
    if selected.is_some_and(custom_model) && !env.contains_key(CONTEXT_WINDOW) {
        env.insert(CONTEXT_WINDOW.into(), Value::String("200000".into()));
    }
}

pub(crate) fn argument_model(args: &[String]) -> Option<&str> {
    let mut selected = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        if arg == "--model" {
            selected = args.next().map(String::as_str);
        } else if let Some(model) = arg.strip_prefix("--model=") {
            selected = Some(model);
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn custom_context_keeps_existing_fallback_and_upstream_model() {
        for model in [
            "gpt-6.1-terra",
            "deepseek-chat",
            "vendor/model",
            "model'$(literal)",
        ] {
            let mut env = json!({"ANTHROPIC_MODEL":model})
                .as_object()
                .unwrap()
                .clone();
            ensure_context_window(&mut env, None);
            assert_eq!(env[CONTEXT_WINDOW], "200000");
            assert_eq!(env["ANTHROPIC_MODEL"], model);
            assert!(!env.contains_key("ANTHROPIC_CUSTOM_MODEL_OPTION"));
        }
    }

    #[test]
    fn custom_context_preserves_provider_window_and_other_settings() {
        for window in [json!("128000"), json!("400000"), json!(128000)] {
            let mut env = json!({"ANTHROPIC_MODEL":"gpt-6.1-terra", "CLAUDE_CODE_MAX_CONTEXT_TOKENS":window, "ANTHROPIC_CUSTOM_MODEL_OPTION":"my-custom", "ANTHROPIC_DEFAULT_SONNET_MODEL":"role-model"}).as_object().unwrap().clone();
            let original = env.clone();
            ensure_context_window(&mut env, Some("profile-model"));
            assert_eq!(env, original);
        }
    }

    #[test]
    fn custom_context_leaves_native_models_aliases_and_one_m_alone() {
        for model in [
            "",
            "  ",
            "default",
            "best",
            "sonnet",
            "opus[1m]",
            "haiku",
            "fable",
            "opusplan",
            "claude-opus-5-5",
            "anthropic/claude-sonnet-4-6",
            "arn:aws:bedrock:r:a:inference-profile/id",
            "custom[1M]",
        ] {
            let mut env = json!({"ANTHROPIC_MODEL":model})
                .as_object()
                .unwrap()
                .clone();
            let original = env.clone();
            ensure_context_window(&mut env, None);
            assert_eq!(env, original, "{model}");
        }
        let mut empty = Map::new();
        ensure_context_window(&mut empty, None);
        assert!(empty.is_empty());
    }

    #[test]
    fn custom_context_model_argument_handles_overrides_and_prompt_boundary() {
        for args in [
            vec!["--model", "gpt-6.1-terra"],
            vec!["--model=gpt-6.1-terra"],
            vec!["--model", "first", "--model", "gpt-6.1-terra"],
        ] {
            assert_eq!(
                argument_model(&args.into_iter().map(String::from).collect::<Vec<_>>()),
                Some("gpt-6.1-terra")
            );
        }
        assert_eq!(
            argument_model(&["--".into(), "--model=gpt-6.1-terra".into()]),
            None
        );
    }
}
