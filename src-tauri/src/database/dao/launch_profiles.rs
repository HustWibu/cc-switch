//! Launch profiles and hashed process credentials. All mutations are transactional.
use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::services::launch_profiles::{fingerprint, provider, LaunchProfile};
use rusqlite::params;

fn db_error(error: rusqlite::Error) -> AppError {
    AppError::Database(error.to_string())
}
fn decode(text: &str) -> Result<LaunchProfile, AppError> {
    serde_json::from_str(text)
        .map_err(|_| AppError::Message("Invalid launch profile record".into()))
}

impl Database {
    pub fn list_launch_profiles(&self) -> Result<Vec<LaunchProfile>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare("SELECT payload FROM launch_profiles ORDER BY launcher")
            .map_err(db_error)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(db_error)?;
        rows.map(|r| decode(&r.map_err(db_error)?)).collect()
    }

    pub fn save_launch_profile(
        &self,
        mut profile: LaunchProfile,
    ) -> Result<LaunchProfile, AppError> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn.transaction().map_err(db_error)?;
        // Timestamps and revisions are owned by persistence, never by IPC input.
        profile.revision = 0;
        profile.created_at = 0;
        profile.is_default = false;
        // Reject nested paths too: profiles must not share a user storage subtree.
        {
            let mut stmt = tx
                .prepare("SELECT payload FROM launch_profiles")
                .map_err(db_error)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(db_error)?;
            for row in rows {
                let other = decode(&row.map_err(db_error)?)?;
                if other.id == profile.id {
                    profile.created_at = other.created_at;
                    profile.revision = other.revision;
                    profile.is_default = other.is_default;
                } else if profile.config_dir.starts_with(&other.config_dir)
                    || other.config_dir.starts_with(&profile.config_dir)
                {
                    return Err(AppError::InvalidInput(
                        "Profile directories must not overlap".into(),
                    ));
                }
            }
        }
        let now = chrono::Utc::now().timestamp_millis();
        profile.revision += 1;
        if profile.created_at == 0 {
            profile.created_at = now;
        }
        profile.updated_at = now;
        let payload = serde_json::to_string(&profile)
            .map_err(|_| AppError::Message("Cannot serialize profile".into()))?;
        tx.execute("INSERT INTO launch_profiles(id,launcher,config_dir,payload) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET launcher=excluded.launcher,config_dir=excluded.config_dir,payload=excluded.payload",
            params![profile.id, profile.launcher, profile.config_dir.to_string_lossy(), payload]).map_err(db_error)?;
        tx.execute(
            "DELETE FROM launch_profile_sessions WHERE profile_id=?1",
            [&profile.id],
        )
        .map_err(db_error)?;
        tx.commit().map_err(db_error)?;
        Ok(profile)
    }

    /// Default selection is device-local metadata: no credentials, revisions or sessions change.
    pub fn set_default_launch_profile(&self, id: &str) -> Result<(), AppError> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn.transaction().map_err(db_error)?;
        let mut profiles = {
            let mut stmt = tx
                .prepare("SELECT payload FROM launch_profiles")
                .map_err(db_error)?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(db_error)?;
            rows.map(|r| decode(&r.map_err(db_error)?))
                .collect::<Result<Vec<_>, _>>()?
        };
        if !profiles.iter().any(|p| p.id == id) {
            return Err(AppError::InvalidInput("Unknown default profile".into()));
        }
        for p in &mut profiles {
            let selected = p.id == id;
            if p.is_default != selected {
                p.is_default = selected;
                let text = serde_json::to_string(p)
                    .map_err(|_| AppError::Message("Cannot serialize default profile".into()))?;
                tx.execute(
                    "UPDATE launch_profiles SET payload=?1 WHERE id=?2",
                    params![text, p.id],
                )
                .map_err(db_error)?;
            }
        }
        tx.commit().map_err(db_error)
    }

    pub fn delete_launch_profile(&self, id: &str) -> Result<(), AppError> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn.transaction().map_err(db_error)?;
        tx.execute(
            "DELETE FROM launch_profile_sessions WHERE profile_id=?1",
            [id],
        )
        .map_err(db_error)?;
        tx.execute("DELETE FROM launch_profiles WHERE id=?1", [id])
            .map_err(db_error)?;
        tx.commit().map_err(db_error)
    }

    pub fn issue_launch_session(
        &self,
        hash: &str,
        profile: &LaunchProfile,
        fingerprint: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        let payload: String = conn
            .query_row(
                "SELECT payload FROM launch_profiles WHERE id=?1",
                [&profile.id],
                |r| r.get(0),
            )
            .map_err(db_error)?;
        if decode(&payload)?.revision != profile.revision {
            return Err(AppError::Message(
                "Profile changed during launch; retry".into(),
            ));
        }
        conn.execute("INSERT INTO launch_profile_sessions(token_hash,profile_id,revision,provider_fingerprint) VALUES(?1,?2,?3,?4)", params![hash, profile.id, profile.revision, fingerprint]).map_err(db_error)?;
        Ok(())
    }

    pub(crate) fn revoke_launch_session(&self, hash: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM launch_profile_sessions WHERE token_hash=?1",
            [hash],
        )
        .map_err(db_error)?;
        Ok(())
    }

    pub fn resolve_launch_session(
        &self,
        hash: &str,
    ) -> Result<crate::provider::Provider, AppError> {
        let (profile, revision, expected) = {
            let conn = lock_conn!(self.conn);
            let (text, revision, expected): (String, u64, String) = conn.query_row(
                "SELECT p.payload,s.revision,s.provider_fingerprint FROM launch_profile_sessions s JOIN launch_profiles p ON p.id=s.profile_id WHERE s.token_hash=?1",
                [hash], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_| AppError::Message("Invalid or revoked profile session; relaunch the profile".into()))?;
            (decode(&text)?, revision, expected)
        };
        if profile.revision != revision {
            return Err(AppError::Message("Profile changed; relaunch".into()));
        }
        let selected = provider(self, &profile)?;
        if fingerprint(&selected)? != expected {
            return Err(AppError::Message(
                "Profile provider changed; relaunch to accept the new configuration".into(),
            ));
        }
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::launch_profiles::RoutingMode;
    fn profile() -> LaunchProfile {
        LaunchProfile {
            id: "one".into(),
            name: "One".into(),
            launcher: "claude-one".into(),
            provider_id: "a".into(),
            runtime: crate::services::launch_profiles::LaunchRuntime::Claude,
            config_dir: "/private/profiles/one".into(),
            routing_mode: RoutingMode::Proxy,
            model: None,
            revision: 0,
            created_at: 0,
            is_default: false,
            updated_at: 0,
        }
    }
    #[test]
    fn default_selection_preserves_bound_sessions_and_does_not_fall_back_after_deletion() {
        let db = Database::memory().unwrap();
        let p = crate::provider::Provider::with_id(
            "a".into(),
            "Mock".into(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude", &p).unwrap();
        let a = db.save_launch_profile(profile()).unwrap();
        let mut other = profile();
        other.id = "two".into();
        other.launcher = "opencode".into();
        other.config_dir = "/private/profiles/two".into();
        let b = db.save_launch_profile(other).unwrap();
        db.issue_launch_session("session", &a, &fingerprint(&p).unwrap())
            .unwrap();
        db.set_default_launch_profile(&a.id).unwrap();
        assert_eq!(
            crate::services::launch_profiles::dispatch_target(&db, &[])
                .unwrap()
                .0,
            a.id
        );
        db.set_default_launch_profile(&b.id).unwrap();
        assert_eq!(db.resolve_launch_session("session").unwrap().id, "a");
        assert_eq!(
            crate::services::launch_profiles::dispatch_target(&db, &[])
                .unwrap()
                .0,
            b.id
        );
        let args = vec![
            std::ffi::OsString::from("claude-one"),
            std::ffi::OsString::from("space argument"),
        ];
        let (id, forwarded) =
            crate::services::launch_profiles::dispatch_target(&db, &args).unwrap();
        assert_eq!(id, a.id);
        assert_eq!(forwarded, vec![std::ffi::OsString::from("space argument")]);
        assert!(
            crate::services::launch_profiles::dispatch_target(&db, &["missing".into()]).is_err()
        );
        let args = vec!["--".into(), "a prompt with spaces".into()];
        assert_eq!(
            crate::services::launch_profiles::dispatch_target(&db, &args)
                .unwrap()
                .1,
            vec![std::ffi::OsString::from("a prompt with spaces")]
        );
        db.delete_launch_profile(&b.id).unwrap();
        assert!(crate::services::launch_profiles::dispatch_target(&db, &[]).is_err());
        assert!(db.set_default_launch_profile("missing").is_err());
        assert!(!db
            .list_launch_profiles()
            .unwrap()
            .iter()
            .any(|p| p.is_default));
    }
    #[test]
    fn launch_profiles_survive_database_reopen_and_stay_local_during_sync() {
        fn open(path: &std::path::Path) -> Database {
            let db = Database {
                conn: std::sync::Mutex::new(rusqlite::Connection::open(path).unwrap()),
                log_count_cache: std::sync::Mutex::new(None),
            };
            db.create_tables().unwrap();
            db
        }
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("profiles.db");
        let db = open(&path);
        let p = crate::provider::Provider::with_id(
            "a".into(),
            "Mock".into(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude", &p).unwrap();
        let saved = db.save_launch_profile(profile()).unwrap();
        db.issue_launch_session("unique-session-digest", &saved, &fingerprint(&p).unwrap())
            .unwrap();
        db.set_default_launch_profile(&saved.id).unwrap();
        let sync = db.export_sql_string_for_sync().unwrap();
        assert!(!sync.contains("unique-session-digest"));
        assert!(!sync.contains("claude-one"));
        drop(db);
        let db = open(&path);
        assert_eq!(db.list_launch_profiles().unwrap()[0].provider_id, "a");
        assert!(db.list_launch_profiles().unwrap()[0].is_default);
        assert_eq!(
            db.resolve_launch_session("unique-session-digest")
                .unwrap()
                .id,
            "a"
        );
    }
    #[test]
    fn launch_profile_crud_preserves_files_and_revokes_sessions() {
        let db = Database::memory().unwrap();
        let p = crate::provider::Provider::with_id(
            "a".into(),
            "Mock".into(),
            serde_json::json!({}),
            None,
        );
        db.save_provider("claude", &p).unwrap();
        let first = db.save_launch_profile(profile()).unwrap();
        db.issue_launch_session("hash", &first, &fingerprint(&p).unwrap())
            .unwrap();
        assert_eq!(db.resolve_launch_session("hash").unwrap().id, "a");
        assert!(db.resolve_launch_session("unknown").is_err());
        let mut changed = first;
        changed.name = "Changed".into();
        let saved = db.save_launch_profile(changed).unwrap();
        assert_eq!(saved.revision, 2);
        assert!(db.resolve_launch_session("hash").is_err());
        assert_eq!(db.list_launch_profiles().unwrap()[0].name, "Changed");
        db.delete_launch_profile("one").unwrap();
        assert!(db.list_launch_profiles().unwrap().is_empty());
    }
    #[test]
    fn launch_profile_sessions_never_follow_provider_edits() {
        let db = Database::memory().unwrap();
        let mut p = crate::provider::Provider::with_id(
            "a".into(),
            "Mock".into(),
            serde_json::json!({"env":{"ANTHROPIC_BASE_URL":"http://a.invalid"}}),
            None,
        );
        db.save_provider("claude", &p).unwrap();
        let profile = db.save_launch_profile(profile()).unwrap();
        db.issue_launch_session("hash", &profile, &fingerprint(&p).unwrap())
            .unwrap();
        p.settings_config = serde_json::json!({"env":{"ANTHROPIC_BASE_URL":"http://b.invalid"}});
        db.save_provider("claude", &p).unwrap();
        assert!(db.resolve_launch_session("hash").is_err());
    }
    #[test]
    fn launch_profiles_reject_alias_and_directory_collisions() {
        let db = Database::memory().unwrap();
        db.save_launch_profile(profile()).unwrap();
        let mut other = profile();
        other.id = "two".into();
        other.config_dir = "/private/profiles/two".into();
        assert!(db.save_launch_profile(other.clone()).is_err());
        other.launcher = "claude-two".into();
        other.config_dir = "/private/profiles/one/nested".into();
        assert!(db.save_launch_profile(other).is_err());
    }
}
