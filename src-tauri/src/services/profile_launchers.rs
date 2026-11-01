//! User-installed launchers contain only a native binary path and immutable profile ID.
use super::launch_profiles::{self, LaunchProfile};
use crate::{database::Database, error::AppError};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherStatus {
    pub path: PathBuf,
    pub installed: bool,
    pub on_path: bool,
    pub command: String,
}
fn directory() -> PathBuf {
    crate::config::get_home_dir().join(".local/bin")
}
fn marker(profile: &LaunchProfile) -> String {
    format!("# CC_SWITCH_PROFILE:{}\n", profile.id)
}
fn owned(path: &Path, profile: &LaunchProfile) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
        && std::fs::read_to_string(path)
            .is_ok_and(|s| s.starts_with(&format!("#!/bin/sh\n{}", marker(profile))))
}
pub fn status(profile: &LaunchProfile) -> LauncherStatus {
    let root = dispatcher_status();
    if root.installed {
        return LauncherStatus {
            command: format!("cc-switch {}", profile.launcher),
            ..root
        };
    }
    let path = directory().join(&profile.launcher);
    LauncherStatus {
        installed: owned(&path, profile),
        path,
        on_path: root.on_path,
        command: profile.launcher.clone(),
    }
}
const DISPATCHER_MARKER: &str = "#!/bin/sh\n# CC_SWITCH_DISPATCHER\n";
pub fn dispatcher_status() -> LauncherStatus {
    let path = directory().join("cc-switch");
    let installed = std::fs::symlink_metadata(&path)
        .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
        && std::fs::read_to_string(&path).is_ok_and(|s| s.starts_with(DISPATCHER_MARKER));
    LauncherStatus {
        path,
        installed,
        on_path: std::env::var_os("PATH")
            .is_some_and(|v| std::env::split_paths(&v).any(|p| p == directory())),
        command: "cc-switch".into(),
    }
}

pub fn install_dispatcher() -> Result<LauncherStatus, AppError> {
    install_dispatcher_at(
        &directory(),
        &std::env::current_exe().map_err(|e| AppError::Message(e.to_string()))?,
    )?;
    enable_shell_path(&crate::config::get_home_dir(), &directory())?;
    Ok(dispatcher_status())
}

pub(crate) fn install_dispatcher_at(dir: &Path, exe: &Path) -> Result<(), AppError> {
    #[cfg(not(unix))]
    {
        let _ = (dir, exe);
        return Err(AppError::Message(
            "CLI installation supports macOS and Linux".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for ancestor in dir.ancestors() {
            if std::fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(AppError::InvalidInput(
                    "CLI installation directory must not contain symlinks".into(),
                ));
            }
        }
        launch_profiles::prepare_directory(dir)?;
        let path = dir.join("cc-switch");
        let is_owned = std::fs::symlink_metadata(&path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
            && std::fs::read_to_string(&path).is_ok_and(|s| s.starts_with(DISPATCHER_MARKER));
        if std::fs::symlink_metadata(&path).is_ok() && !is_owned {
            return Err(AppError::InvalidInput(
                "An unrelated cc-switch command already exists; it was not overwritten".into(),
            ));
        }
        if let Some(value) = std::env::var_os("PATH") {
            for entry in std::env::split_paths(&value) {
                let candidate = entry.join("cc-switch");
                if candidate != path && candidate.exists() {
                    return Err(AppError::InvalidInput("Another cc-switch command is on PATH; resolve the conflict before installing".into()));
                }
            }
        }
        let body = format!(
            r#"{DISPATCHER_MARKER}if [ "${{1-}}" = profile ]; then
  shift
  exec {} profile "$@"
fi
exec {} profile dispatch "$@"
"#,
            quote(&exe.to_string_lossy()),
            quote(&exe.to_string_lossy())
        );
        let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| AppError::io(dir, e))?;
        use std::io::Write;
        file.write_all(body.as_bytes())
            .map_err(|e| AppError::io(&path, e))?;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o700))
            .map_err(|e| AppError::io(&path, e))?;
        file.as_file()
            .sync_all()
            .map_err(|e| AppError::io(&path, e))?;
        file.persist(&path)
            .map_err(|e| AppError::io(&path, e.error))?;
        Ok(())
    }
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn install(db: &Database, id: &str) -> Result<LauncherStatus, AppError> {
    let profile = db
        .list_launch_profiles()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::InvalidInput("Unknown profile".into()))?;
    install_dispatcher()?;
    if !matches!(profile.launcher.as_str(), "claude" | "codex" | "opencode") {
        install_at(
            &profile,
            &directory(),
            &std::env::current_exe().map_err(|e| AppError::Message(e.to_string()))?,
        )?;
    }
    // A managed, idempotent PATH block applies to newly opened interactive shells.
    enable_shell_path(&crate::config::get_home_dir(), &directory())?;
    Ok(status(&profile))
}

fn enable_shell_path(home: &Path, dir: &Path) -> Result<(), AppError> {
    for name in [".zshrc", ".bashrc", ".bash_profile"] {
        let path = home.join(name);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(AppError::InvalidInput(format!(
                "Launcher installed, but {} is a symlink. Add ~/.local/bin to PATH yourself.",
                path.display()
            )));
        }
        let mut text = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(AppError::io(&path, e)),
        };
        if !text.contains("# CC Switch profile launchers") {
            text.push_str(&format!(
                "\n# CC Switch profile launchers\nexport PATH={}:\"$PATH\"\n",
                quote(&dir.to_string_lossy())
            ));
            crate::config::atomic_write_private(&path, text.as_bytes())?;
        }
    }
    Ok(())
}

pub fn uninstall(db: &Database, id: &str) -> Result<(), AppError> {
    let profile = db
        .list_launch_profiles()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::InvalidInput("Unknown profile".into()))?;
    let path = directory().join(&profile.launcher);
    if owned(&path, &profile) {
        std::fs::remove_file(&path).map_err(|e| AppError::io(&path, e))?;
    }
    Ok(())
}

pub(crate) fn install_at(profile: &LaunchProfile, dir: &Path, exe: &Path) -> Result<(), AppError> {
    #[cfg(not(unix))]
    {
        let _ = (profile, dir, exe);
        return Err(AppError::Message(
            "Command installation currently supports macOS and Linux".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        launch_profiles::validate(profile, &crate::config::get_claude_config_dir())?;
        if matches!(profile.launcher.as_str(), "claude" | "codex" | "opencode") {
            return Err(AppError::InvalidInput(
                "Use cc-switch <profile> for names matching native CLI binaries".into(),
            ));
        }
        for path in dir.ancestors() {
            if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(AppError::InvalidInput(
                    "Launcher installation directory must not contain symlinks".into(),
                ));
            }
        }
        launch_profiles::prepare_directory(dir)?;
        let path = dir.join(&profile.launcher);
        if std::fs::symlink_metadata(&path).is_ok() && !owned(&path, profile) {
            return Err(AppError::InvalidInput(
                "A different command already exists; choose another launcher name".into(),
            ));
        }
        if let Some(value) = std::env::var_os("PATH") {
            for entry in std::env::split_paths(&value) {
                let candidate = entry.join(&profile.launcher);
                if candidate != path && candidate.exists() {
                    return Err(AppError::InvalidInput(
                        "Launcher name conflicts with a command on PATH".into(),
                    ));
                }
            }
        }
        let body = format!(
            "#!/bin/sh\n{}exec {} profile run-id {} \"$@\"\n",
            marker(profile),
            quote(&exe.to_string_lossy()),
            quote(&profile.id)
        );
        let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| AppError::io(dir, e))?;
        use std::io::Write;
        file.write_all(body.as_bytes())
            .map_err(|e| AppError::io(&path, e))?;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o700))
            .map_err(|e| AppError::io(&path, e))?;
        file.as_file()
            .sync_all()
            .map_err(|e| AppError::io(&path, e))?;
        file.persist(&path)
            .map_err(|e| AppError::io(&path, e.error))?;
        // Remove only older aliases carrying this exact profile's ownership marker.
        for entry in std::fs::read_dir(dir).map_err(|e| AppError::io(dir, e))? {
            let entry = entry.map_err(|e| AppError::io(dir, e))?;
            if entry.path() != path && owned(&entry.path(), profile) {
                std::fs::remove_file(entry.path()).map_err(|e| AppError::io(entry.path(), e))?;
            }
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn dispatcher_passes_profile_names_and_arguments_without_shell_evaluation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let binary = root.join("fake native binary");
        std::fs::write(&binary, "#!/bin/sh\nprintf '%s\\n' \"$@\"\nexit 37\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        install_dispatcher_at(&root, &binary).unwrap();
        for (args, expected) in [
            (
                vec!["gpt", "space argument", "$(must-not-run)"],
                "profile\ndispatch\ngpt\nspace argument\n$(must-not-run)\n",
            ),
            (vec!["profile", "run", "gpt"], "profile\nrun\ngpt\n"),
        ] {
            let result = std::process::Command::new(root.join("cc-switch"))
                .args(args)
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(37));
            assert_eq!(String::from_utf8(result.stdout).unwrap(), expected);
        }
        std::fs::write(root.join("cc-switch"), b"unrelated user command").unwrap();
        assert!(install_dispatcher_at(&root, &binary).is_err());
        assert_eq!(
            std::fs::read(root.join("cc-switch")).unwrap(),
            b"unrelated user command"
        );
    }
    #[test]
    fn shell_path_setup_preserves_content_and_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join("bin with spaces");
        let original = "# user config\nexport KEEP=value\n";
        std::fs::write(home.path().join(".zshrc"), original).unwrap();
        enable_shell_path(home.path(), &bin).unwrap();
        enable_shell_path(home.path(), &bin).unwrap();
        let actual = std::fs::read_to_string(home.path().join(".zshrc")).unwrap();
        assert!(actual.starts_with(original));
        assert_eq!(actual.matches("# CC Switch profile launchers").count(), 1);
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                ". {}; printf '%s' \"$PATH\"",
                quote(&home.path().join(".zshrc").to_string_lossy())
            ))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .starts_with(&bin.to_string_lossy().to_string()));
    }
    #[test]
    fn installed_launcher_is_id_bound_executable_and_does_not_overwrite_other_commands() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let mut p: LaunchProfile=serde_json::from_value(serde_json::json!({"id":"immutable","name":"Test","launcher":"test-profile-command-unique","providerId":"provider","configDir":root.join("config"),"routingMode":"official","model":null})).unwrap();
        install_at(
            &p,
            &root,
            &PathBuf::from("/Applications/CC Switch.app/Contents/MacOS/cc-switch"),
        )
        .unwrap();
        let text = std::fs::read_to_string(root.join(&p.launcher)).unwrap();
        assert!(text.contains("profile run-id 'immutable' \"$@\""));
        let fake = root.join("fake native binary");
        std::fs::write(&fake, "#!/bin/sh\nprintf '%s\\n' \"$@\"\nexit 37\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        install_at(&p, &root, &fake).unwrap();
        let output = std::process::Command::new(root.join(&p.launcher))
            .arg("space argument")
            .arg("$(must-not-run)")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(37));
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "profile\nrun-id\nimmutable\nspace argument\n$(must-not-run)\n"
        );
        let old = p.launcher.clone();
        p.launcher = "renamed-profile-command-unique".into();
        install_at(&p, &root, Path::new("/binary")).unwrap();
        assert!(!root.join(old).exists());
        std::fs::write(root.join("other-command"), b"user file").unwrap();
        p.launcher = "other-command".into();
        assert!(install_at(&p, &root, Path::new("/binary")).is_err());
        assert_eq!(
            std::fs::read(root.join("other-command")).unwrap(),
            b"user file"
        );
    }
}
