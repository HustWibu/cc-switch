# Repository analysis

Inspected README.md, CONTRIBUTING.md, Cargo.toml, package.json and docs/guides/claude-codex-routing-guide-en.md. No AGENTS.md, CLAUDE.md or CONTEXT.md was found in the checkout. CONTRIBUTING.md contains the architecture and testing instructions; docs/design-system.html supplies UI design guidance.

## Existing components

- React/TypeScript -> TanStack Query -> src/lib/api -> Tauri invoke. Rust commands -> services -> SQLite DAO and live file writers.
- src-tauri/src/services/provider/: ProviderService handles provider CRUD and switching. Stable IDs are scoped by tool in database/dao/providers.rs (get_provider_by_id).
- services/provider/claude_direct.rs: switch_to/reapply/write use ClaudeProjection/direct_patch, private LiveFile and mode::operation. Only provider-owned fields are patched, under app write locking; existing user fields survive. Official switches can also move login via claude_official::with_login_switch.
- config.rs: get_claude_config_dir uses the device override or ~/.claude, rather than process CLAUDE_CONFIG_DIR. Settings and MCP paths have distinct resolution functions. Existing global management must not target profile directories.
- services/claude_account/: isolated official accounts already have CLAUDE_CONFIG_DIR directories; login::clear_account_env removes inherited routing/auth variables. shared.rs synchronizes selected settings, Skills and MCP for those accounts. Reusing that automatic synchronization for new profiles would violate the requested independent initial configuration.
- commands/misc.rs: claude_official_launch supplies per-account launch environments. This already permits concurrent official accounts, but does not provide named persistent profiles for arbitrary proxy providers.
- services/profile.rs and database/dao/profiles.rs: existing project snapshots apply ProviderService::switch globally. They are not launch profiles.
- proxy/server.rs: one axum listener, routes per tool. proxy/handler_context.rs: RequestContext::new normally resolves mode::current and ProviderRouter per request, including failover. Thus ordinary routing is global per tool, not pinned per process or connection.
- mode/stack.rs: StackTarget plus RequestContext's is_stack/create_forwarder single-target path disables failover and routing-state changes. Reuse this path for profile requests, without requiring Stack mode or encoded model IDs.
- proxy/handlers.rs: ordinary Claude endpoints have no profile identity authentication. Claude Desktop has separate constant-time gateway authentication. PROXY_MANAGED in normal live configuration is a placeholder, not a per-profile security boundary.
- proxy/providers/codex_oauth_auth.rs: existing managed OAuth accounts and token refresh. Provider meta authBinding selects accounts; profile routing must require explicit bindings for OAuth rather than following mutable defaults.
- session_manager/: existing session scanning/resume is tool-directory based; scanning arbitrary new profile directories will require additional integration.
- database/schema.rs: providers, provider_endpoints, settings, profiles, proxy_config, provider_health and proxy_request_logs. Schema migration/version is maintained here; CONTRIBUTING asks contributors not to bump SCHEMA_VERSION.
- Backend unit tests sit next to source, integration tests in src-tauri/tests. Frontend vitest/MSW tests are in tests and src. CC_SWITCH_TEST_HOME must isolate backend test runs.

## Claude Code boundary

Official references: https://code.claude.com/docs/en/env-vars, https://code.claude.com/docs/en/authentication, https://code.claude.com/docs/en/settings.

CLAUDE_CONFIG_DIR overrides user configuration. Credentials may use a directory-specific macOS Keychain entry or .credentials.json; do not copy these. CLAUDE_SECURESTORAGE_CONFIG_DIR must not leak from the parent process. User history, plugins, Skills and user MCP belong to user storage; project .claude settings, .mcp.json, project instructions and organization policy remain applicable. Directory isolation is not an OS sandbox or a promise that project configuration cannot override routing. Verify loaded settings with /status on the installed Claude version.

## Gaps and risks

No named arbitrary-provider isolated launcher lifecycle exists. New profile routes require fail-closed authentication and no failover. Provider/account edits must revoke sessions rather than retarget them. Local processes running as the same OS user can inspect environments and files; loopback does not isolate mutually untrusted users. Real account/CLI behavior cannot be proven using mock providers alone.

Provider records have no general disabled flag. Profile validation rejects missing providers and official/proxy mode mismatches; bound OAuth account availability is checked by the existing OAuth manager during forwarding. `commands/proxy.rs::start_proxy_server` starts only the server and does not take over live configuration; the new dialog reuses this operation. Profile storage and session digests are local-only tables in the existing sync exclusion/preservation lists. Indexed profile authentication uses the existing database mutex; no new global request-path lock or polling is introduced.
