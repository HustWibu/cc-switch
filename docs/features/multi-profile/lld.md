# Low-level design

New launch_profiles service owns LaunchProfile (id, name, launcher, providerId, configDir, routingMode, model, revision, createdAt, updatedAt), validation, CRUD and launch environment. Add launch_profiles and launch_profile_sessions tables through idempotent schema creation. Unique launcher/config directory constraints prevent collisions. Session rows hold token hash, profile ID, revision and provider fingerprint; never raw tokens. No foreign key cascade is required: missing profile explicitly invalidates old sessions.

Tauri commands list_launch_profiles, save_launch_profile, delete_launch_profile and launch_profile_terminal extend commands registration. TypeScript wrappers live in src/lib/api/launchProfiles.ts. A focused dialog on the Claude provider page extends existing shadcn components. Duplication creates a new ID, directory and alias through save; deletion retains all user files.

The existing native binary handles `profile run <alias> [arguments...]` before Tauri initialization. Unix exec preserves terminal, signals, cwd, byte arguments and exit codes. Clear inherited Claude authentication/routing variables using the existing account helper, then set both config and credential directory explicitly. Routed processes receive only local session credentials. The existing provider data supplies actual upstream secrets at forwarding time.

The dedicated /profiles/v1/messages handler authenticates before common forwarding, strips the path prefix, and injects an existing StackTarget. Common forwarding bypasses global model resolution when that target exists. Profile requests never consult the global failover queue. Unknown/missing credentials are authentication errors. Missing/edited providers invalidate existing sessions. Require explicit managed account bindings for OAuth providers so account defaults cannot silently change.

Configuration paths must be absolute, unique, free of parent traversal and symlink components, outside default Claude storage, and outside another profile's subtree. New directories are private on Unix; existing directories are preserved and never recursively deleted. Launcher names use an ASCII allowlist. No shell interpolation of profile-controlled text without quoting.

Before launch, `check_settings` parses existing JSONC through `jsonc_document::JsoncDocument` and rejects endpoint/authentication overrides in profile settings and ancestor project settings through the repository root. It skips home-level user storage, preserves all bytes, and permits model-only choices. Existing ancestors are canonicalized for directory overlap checks, including case aliases on macOS. Enterprise policies and settings changed after launch remain outside this startup check.

## UI-installed commands and additional runtimes

`LaunchProfile.runtime` is a serde-defaulted enum (`claude`, `codex`, `opencode`); old payloads deserialize as Claude. Provider IDs are resolved in the matching tool namespace. `RoutingMode::Direct` supplements the existing official/proxy modes without changing existing table schemas.

`services/profile_launchers.rs` installs private executable wrappers using temporary files and rename, with an ownership marker and immutable profile ID. `profile run-id` resolves that ID before forwarding to the existing launcher flow. Command installation validates existing PATH collisions and rejects symlink installation ancestors. Shell PATH setup is idempotent. `install_profile_launcher` and `profile_launcher_status` extend the Tauri API; profile deletion also removes its owned current command.

The native Codex branch reuses `provider/codex_terminal::build` against the isolated directory's config and injects credentials via env. The OpenCode branch reuses `opencode_config::provider_fragment` and format detection; runtime overrides stay in the child environment. Neither branch switches global providers or copies authentication.

`ProviderLauncherFields.tsx` integrates command configuration into AddProviderDialog and EditProviderDialog. `useProviderActions::addProvider` now returns the persisted provider object so profiles bind to the exact generated ID. Empty launcher input invokes no profile side effects. A provider save remains successful if the subsequent launcher operation fails, with an explicit partial-success toast and recovery through the management dialog.

## Default profile selection

`LaunchProfile.is_default` (`isDefault` in JSON) defaults to false for old payloads. `Database::set_default_launch_profile` updates the existing local payload rows in a single transaction, retaining IDs, timestamps, revisions and bound sessions. Normal save preserves the server-owned default flag; duplicated/new records cannot claim it through client payloads. No table or schema-version change is needed. Existing cloud exclusions keep the choice local to the device.

New Tauri commands: `set_default_launch_profile`, `install_profile_dispatcher`, `profile_dispatcher_status`. The dialog renders the global default summary, row badge, selection button and dispatcher install action. The dispatcher wrapper contains only the native executable path and forwards all argv into `profile dispatch`. No args selects the configured default ID; a name selects that profile explicitly. Flags and slash commands go to the default; `--` explicitly forwards a default-profile prompt. Unknown names, missing defaults and inconsistent multiple defaults fail clearly.

`command_by_id` launches from a single profile snapshot, avoiding an ID-to-alias-to-profile lookup race. Terminal launch and ID-bound wrappers use this path. Namespace names matching native binaries are supported without installing replacement `claude`, `codex` or `opencode` executables.
