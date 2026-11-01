# Reviewable implementation phases

1. Repository analysis: trace actual configuration, account, persistence and forwarding boundaries (repository-analysis.md).
2. Design: agree fail-closed session semantics and credential boundary (hld.md/lld.md). Reuse the existing single-target forwarding path.
3. Persistence and commands: additive schema, validated CRUD, provider association, isolated database tests.
4. Launcher: native pre-GUI entry point, private directories, environment sanitation, argument/exit preservation; test with a fake Claude binary.
5. Routing: dedicated authenticated endpoint, immutable session associations, fail-closed provider/profile mutation; test concurrent mock traffic and global switching.
6. UI: focused management dialog, provider selection, duplicate/remove and terminal launch; all four maintained locales.
7. Verification: isolated cargo tests, fmt/clippy, TypeScript, frontend tests/build and unsigned desktop build. Record actual results and gaps in testing.md.

Migration: additive tables only, no existing credentials or live configuration rewritten. Rollback: use prior binary; retain profile directories and tables until sessions stop. Drop only the new tables if desired after backup; do not delete user-created directories or existing profiles table. No SCHEMA_VERSION bump is proposed; maintainers must review additive schema handling before release.

## Source change summary

| Existing component extended | Change |
| --- | --- |
| `src-tauri/src/database/schema.rs` | Idempotent additive launch profile/session tables and lookup index. |
| `src-tauri/src/database/dao/launch_profiles.rs` (new) | Atomic CRUD, uniqueness/overlap checks, session issuance/resolution and persistence tests. |
| `src-tauri/src/database/backup.rs` | Preserve local launch records during sync imports and exclude them from remote exports. |
| `src-tauri/src/services/launch_profiles.rs` (new) | Profile validation, private directory creation, inherited environment cleanup, conflicting settings detection, provider fingerprints and native CLI behavior. |
| `src-tauri/src/commands/launch_profiles.rs` (new) | Thin Tauri CRUD commands and quoted macOS terminal launch. |
| `src-tauri/src/main.rs`, `lib.rs`, module registries | Dispatch profile CLI before GUI initialization; register commands. |
| `src-tauri/src/proxy/server.rs`, `handlers.rs` | Authenticated profile route; reuse existing StackTarget single-provider forwarding; mock routing/restart tests. |
| `src/lib/api/launchProfiles.ts` (new) | Typed Tauri API. |
| `src/components/profiles/LaunchProfilesDialog.tsx` (new), `src/App.tsx` | Focused Claude-page management dialog and server-only proxy start. |
| `src/i18n/locales/{en,zh,zh-TW,ja}.json` | Localized profile UI. |
| `src-tauri/tests/launch_profiles_cli.rs` (new) | Actual native executable with concurrent fake-Claude children, including two launches of the same profile. |
| `tests/components/LaunchProfilesDialog.test.tsx` (new) | Duplicate isolation, visible errors and selected-profile removal. |

No new production dependency, background poller, proxy listener or separate proxy process is introduced.

## Evidence and completion boundary

Phases 1–2: analysis and design documents exist with source paths and diagrams. Phases 3–5: ten focused backend tests, native launcher integration and local mock routing tests passed. Phase 6: three dialog tests, all maintained locales, strict TypeScript and renderer/desktop builds passed. Phase 7: 4,340 backend unit tests, 130 relevant integration tests and 3,477 frontend tests passed with environment adjustments detailed in testing.md; formatting and Clippy passed.

Live subscription login, installed Claude Code behavior, terminal interaction and end-user manual acceptance remain outstanding. The feature is implemented and mock-verified; this is not a claim that the user-specific Definition of Done has been fully demonstrated. No real subscription quota was used for automated testing.

## Follow-up: configure commands directly in provider UI

Implemented `runtime` with backward-compatible Claude defaults, provider-form launcher fields, exact persisted-provider ID propagation, managed command installation/status, save-and-install/copy actions, and direct native Codex/OpenCode launch plans. New implementation: `services/profile_launchers.rs` and `components/profiles/ProviderLauncherFields.tsx`; existing Codex override builder is exposed within the crate for reuse. No new database schema or dependency is required.

The installer uses an immutable ID, rejects unrelated existing commands, retains shell config content, and is tested for quoting, literal arguments, exit status, rename cleanup and idempotent PATH setup. Native integration tests use fake Claude/Codex/OpenCode binaries and temporary databases. Actual OpenCode `debug config` was also verified without a model request.

## Follow-up: default profile

Added a server-owned `isDefault` flag to local profile payloads, transactional default selection and an installed `cc-switch` dispatcher. Default selection only changes launch metadata and preserves existing sessions/revisions. The UI exposes selection, badge, summary and command installation. Both explicit profile names and default launches resolve to immutable IDs before creating a process. The app's no-argument GUI startup is unchanged because the installed wrapper supplies the internal dispatch prefix.

Verification covers persisted defaults after reopen, changing defaults without invalidating existing profile credentials, deletion/unknown-name failure, script argument/exit behavior and preservation of original Claude settings bytes. The frontend choice uses the profile ID and never invokes provider switching.
