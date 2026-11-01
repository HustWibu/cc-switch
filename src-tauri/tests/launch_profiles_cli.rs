//! End-to-end native launcher test; the real Claude binary is never invoked.
#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

#[test]
fn native_profile_launchers_preserve_concurrent_environments_arguments_and_exit_codes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let binary = env!("CARGO_BIN_EXE_cc-switch");
    let original_dir = root.join(".claude");
    std::fs::create_dir(&original_dir).unwrap();
    let original=br#"{"env":{"ANTHROPIC_BASE_URL":"http://global.invalid","ANTHROPIC_AUTH_TOKEN":"mock-original-secret"},"keep":true}"#;
    std::fs::write(original_dir.join("settings.json"), original).unwrap();
    let initialize = Command::new(binary)
        .args(["profile", "run", "missing"])
        .env("CC_SWITCH_TEST_HOME", &root)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(initialize.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&initialize.stderr).contains("Unknown launch profile"));
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let fake = bin.join("claude");
    std::fs::write(&fake, "#!/bin/sh\nprintf '%s\\n' \"$CLAUDE_CONFIG_DIR\" \"$CLAUDE_SECURESTORAGE_CONFIG_DIR\" \"$PWD\" \"${ANTHROPIC_BASE_URL-unset}\" \"$@\"\nexit 37\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
    let conn = rusqlite::Connection::open(root.join(".cc-switch/cc-switch.db")).unwrap();
    conn.execute("INSERT INTO providers(id,app_type,name,settings_config,category) VALUES('mock-official','claude','Mock official','{}','official')", []).unwrap();
    for name in ["a", "b"] {
        let path = root.join(format!("profile-{name}"));
        let payload = serde_json::json!({"id":name,"name":name,"launcher":if name=="a"{"gpt"}else{"claude-pro"},"providerId":"mock-official","configDir":path,"routingMode":"official","isDefault":name=="a","model":null,"revision":1,"createdAt":1,"updatedAt":1});
        conn.execute(
            "INSERT INTO launch_profiles(id,launcher,config_dir,payload) VALUES(?1,?2,?3,?4)",
            rusqlite::params![
                name,
                if name == "a" { "gpt" } else { "claude-pro" },
                path.to_str().unwrap(),
                payload.to_string()
            ],
        )
        .unwrap();
    }
    drop(conn);
    let mut children = Vec::new();
    for (index, name) in ["a", "b", "a"].into_iter().enumerate() {
        let selector = if index == 2 {
            "--".to_string()
        } else {
            if name == "a" {
                "gpt".to_owned()
            } else {
                "claude-pro".to_owned()
            }
        };
        let child = Command::new(binary)
            .args([
                "profile",
                "dispatch",
                &selector,
                "space argument",
                "$(must-not-run)",
                "--resume",
                "session-id",
            ])
            .env("CC_SWITCH_TEST_HOME", &root)
            .env("PATH", &bin)
            .env("ANTHROPIC_BASE_URL", "http://must-not-inherit.invalid")
            .current_dir(&root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        children.push((name, child));
    }
    for (name, child) in children {
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(37),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = format!("{0}/profile-{name}\n{0}/profile-{name}\n{0}\nunset\nspace argument\n$(must-not-run)\n--resume\nsession-id\n", root.display());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        assert!(!root
            .join(format!("profile-{name}/.credentials.json"))
            .exists());
    }
    assert_eq!(
        std::fs::read(original_dir.join("settings.json")).unwrap(),
        original
    );
}

#[test]
fn codex_and_opencode_native_profiles_use_independent_runtime_configurations() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let binary = env!("CARGO_BIN_EXE_cc-switch");
    let init = Command::new(binary)
        .args(["profile", "run", "missing"])
        .env("CC_SWITCH_TEST_HOME", &root)
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(init.status.code(), Some(1));
    let bin = root.join("bin");
    std::fs::create_dir(&bin).unwrap();
    let codex = bin.join("codex");
    std::fs::write(&codex,"#!/bin/sh\n[ \"$CC_SWITCH_CODEX_API_KEY\" = mock-key ] || exit 99\nprintf '%s\\n' \"$CODEX_HOME\" \"$@\"\nexit 37\n").unwrap();
    let opencode = bin.join("opencode");
    std::fs::write(&opencode,"#!/bin/sh\nprintf '%s\\n' \"$XDG_DATA_HOME\" \"$OPENCODE_CONFIG_CONTENT\" \"$@\"\nexit 38\n").unwrap();
    for file in [&codex, &opencode] {
        std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let conn = rusqlite::Connection::open(root.join(".cc-switch/cc-switch.db")).unwrap();
    let codex_config = serde_json::json!({"auth":{"OPENAI_API_KEY":"mock-key"},"config":"model = 'mock-model'\nmodel_provider = 'mock'\n[model_providers.mock]\nname = 'Mock'\nbase_url = 'https://mock.invalid/v1'\nwire_api = 'responses'\n"});
    let open_config = serde_json::json!({"npm":"@ai-sdk/openai-compatible","options":{"baseURL":"https://other.invalid/v1","apiKey":"other-mock-key"},"models":{"mock-model":{}}});
    for (runtime, config) in [("codex", codex_config), ("opencode", open_config)] {
        conn.execute(
            "INSERT INTO providers(id,app_type,name,settings_config) VALUES('shared-id',?1,?1,?2)",
            rusqlite::params![runtime, config.to_string()],
        )
        .unwrap();
        let p = serde_json::json!({"id":runtime,"name":runtime,"launcher":if runtime=="opencode"{"opencode"}else{"codex-work"},"providerId":"shared-id","runtime":runtime,"configDir":root.join(runtime),"routingMode":"direct","model":null,"revision":1,"createdAt":1,"updatedAt":1});
        conn.execute(
            "INSERT INTO launch_profiles VALUES(?1,?2,?3,?4)",
            rusqlite::params![
                runtime,
                if runtime == "opencode" {
                    "opencode"
                } else {
                    "codex-work"
                },
                root.join(runtime).to_str().unwrap(),
                p.to_string()
            ],
        )
        .unwrap();
    }
    drop(conn);
    let children: Vec<_> = ["codex", "opencode"]
        .into_iter()
        .map(|runtime| {
            (
                runtime,
                Command::new(binary)
                    .args([
                        "profile",
                        "dispatch",
                        if runtime == "opencode" {
                            "opencode"
                        } else {
                            "codex-work"
                        },
                        "space argument",
                    ])
                    .env("CC_SWITCH_TEST_HOME", &root)
                    .env("PATH", &bin)
                    .env("CODEX_HOME", "/wrong")
                    .env("OPENCODE_CONFIG_CONTENT", "{\"model\":\"wrong\"}")
                    .current_dir(&root)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            )
        })
        .collect();
    for (runtime, child) in children {
        let output = child.wait_with_output().unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            output.status.code(),
            Some(if runtime == "codex" { 37 } else { 38 }),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.starts_with(&root.join(runtime).to_string_lossy().to_string()));
        assert!(text.contains("space argument"));
        if runtime == "codex" {
            assert!(!text.contains("mock-key"));
            assert!(text.contains("mock.invalid"));
        } else {
            let config: serde_json::Value =
                serde_json::from_str(text.lines().nth(1).unwrap()).unwrap();
            assert_eq!(config["model"], "shared-id/mock-model");
            assert_eq!(
                config["enabled_providers"],
                serde_json::json!(["shared-id"])
            );
        }
    }
    assert!(!root.join(".codex").exists());
    assert!(!root.join(".config/opencode").exists());
}
