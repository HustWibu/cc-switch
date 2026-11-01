# Verification record

Automated checks use local mock providers and a fake Claude executable; they never authenticate against real subscriptions. Backend commands must run with CC_SWITCH_TEST_HOME pointing to a temporary directory.

## Commands

```sh
corepack pnpm typecheck
corepack pnpm format:check
corepack pnpm test:unit
# For this machine's existing slow Pi form test:
corepack pnpm exec vitest run --maxWorkers=2 --minWorkers=1 --testTimeout=15000
corepack pnpm build:renderer
CC_SWITCH_TEST_HOME="$(mktemp -d)" cargo test --manifest-path src-tauri/Cargo.toml launch_profile --lib
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

## Coverage mapping

- AT-01/02/05/09: handler mock-server test sends concurrent profile traffic to two local upstreams, changes global current provider, checks routing and upstream credential replacement. The native integration test also launches the same profile twice concurrently.
- AT-03: SQLite reopen test verifies persisted profile/provider association and session resolution; cloud export test verifies records remain local.
- AT-04/10: missing/unknown credentials, deleted profiles/providers and provider mutation return errors rather than global fallback. A real proxy listener stop/restart test verifies missing authentication rejection, listener outage and session reuse after restart.
- AT-06: fake executable checks arguments (including shell syntax), working directory, separate config/credential directories and exit code 37 from concurrent children. The native CLI integration test also invokes the built binary directly with a fake executable; literal shell syntax remains an argument.
- AT-07: full backend unit regression plus the relevant provider/proxy/import/profile integration suites passed; the full frontend suite passed with the documented timeout adjustment.
- AT-08: directory preparation test checks existing settings bytes and absence of copied credentials; default path rejection is tested. Full MCP/Skills workflow manual validation remains required.

## Results

TypeScript checking, Prettier, rustfmt and Clippy with `-D warnings` passed. Renderer builds and unsigned debug macOS application bundling passed. The app is at `src-tauri/target/debug/bundle/macos/CC Switch.app`.

Ten profile backend tests passed. The native launcher integration test passed; a separate run of the built binary also demonstrated two concurrent fake-Claude processes, endpoint sanitation, literal arguments, cwd and exit code 37 without creating default Claude storage. Three new dialog tests passed.

The latest full backend unit run passed **4,340 tests**, with **11 ignored**, using a temporary `CC_SWITCH_TEST_HOME` and a subprocess PATH excluding `.bun/bin`. An earlier standard-PATH run failed the existing `commands::misc::tests::omp_search_paths_include_the_bun_global_bin` assertion because this machine already includes that directory. Its focused rerun with the cleaned PATH passed. No unrelated test source was changed.

The frontend default-timeout runs failed the existing Pi provider form test at five seconds: the initial run passed 3,473 other tests; the run including the new dialog tests passed 3,476 others. The Pi file passed all 55 tests in isolation. The final full run with `--maxWorkers=2 --minWorkers=1 --testTimeout=15000` passed **3,477 tests in 271 files**. No test timeout configuration was changed in source.

Selected backend integration suites passed **130 tests**: import/export (19), native launch profiles (1), MCP (39), project profiles (8), provider service (55), proxy commands (1), and Skills (7). These cover the affected legacy seams; they are not a claim that every other integration binary was run.

Final unsigned debug desktop bundle, TypeScript check, formatting checks and Clippy all passed on macOS. No live subscription/interactive CLI verification has been performed, so the complete Definition of Done remains pending that manual acceptance.

## Manual acceptance

Create official and OAuth profiles in a test installation; authenticate separately, check `/status`, launch both terminals, then switch the global UI provider and inspect upstream usage/log attribution. Keep prompt content non-sensitive. Confirm a profile/provider edit invalidates routed sessions. Stop/restart the proxy on the same port; confirm the same session capability still selects its provider. Delete a provider and confirm a visible error. Verify existing MCP, Skills, default login and sessions are unchanged. Test actual keyboard interaction, Ctrl-C and CLI exit codes. No live subscription verification has been performed automatically.

## Repeat the native launcher test

```sh
CC_SWITCH_TEST_HOME="$(mktemp -d)" cargo test --manifest-path src-tauri/Cargo.toml --test launch_profiles_cli
corepack pnpm exec vitest run tests/components/LaunchProfilesDialog.test.tsx
```

The native test replaces PATH only for its child process with a fake `claude`, uses a temporary database, and never reads or writes the real default Claude directory. Actual Anthropic/ChatGPT login, model compatibility, terminal Ctrl-C and resume behavior remain manual acceptance checks. Windows/Linux terminal integration and profile-directory browsing in the session manager are not claimed as verified.

## Final commands for affected integration seams

```sh
CC_SWITCH_TEST_HOME="$(mktemp -d)" cargo test --manifest-path src-tauri/Cargo.toml \
  --test provider_service --test mcp_commands --test skill_sync \
  --test profile_roundtrip --test proxy_commands --test import_export_sync \
  --test launch_profiles_cli
corepack pnpm tauri build --debug --bundles app -c '{"build":{"beforeBuildCommand":"corepack pnpm run build:renderer"},"bundle":{"createUpdaterArtifacts":false}}'
```

For the unit regression on this machine, remove `.bun/bin` from the child process PATH; do not change HOME. This adjustment addresses the pre-existing Bun-path assertion, not profile behavior. Eleven existing ignored Rust tests remained ignored. Production signing/notarization, real Claude login and subscription compatibility were not tested.

## Follow-up verification: provider UI commands and native runtimes

Latest results for the runtime/installer extension:

- **4,344 backend unit tests passed, 11 ignored**, using the same temporary-home and cleaned child PATH arrangement documented above.
- **79 profile-related backend tests passed**, including backward-compatible runtime defaults, tool-scoped provider lookup, installer collision protection, ID-bound scripts, literal arguments/exit codes, rename cleanup and idempotent shell PATH updates.
- **2 native launcher integration tests passed**: concurrent Claude children (including repeated use of one profile), and concurrent native Codex/OpenCode children with separate runtime configuration and preserved arguments. Codex primary API keys are checked in the mock child environment and asserted absent from its argv.
- **3,480 frontend tests passed in 272 files**, using `--maxWorkers=2 --minWorkers=1 --testTimeout=15000`. Focused provider/profile UI and action-hook checks passed 63 tests.
- Strict TypeScript, Prettier, rustfmt, Clippy with warnings denied, renderer build and unsigned debug macOS application bundle all passed.
- Installed CLI versions were checked without model requests: Claude Code 2.1.316, Codex CLI 0.166.0, OpenCode 1.18.15. A real OpenCode `debug config` run with temporary XDG storage and a dummy provider confirmed the inline provider/model and enabled-provider list. No subscription/model request was sent.

The macOS app bundle is `src-tauri/target/debug/bundle/macos/CC Switch.app`. Generated commands and shell-init changes were exercised only in temporary test directories; no commands were installed in the real user's home during automated testing. Native official Codex login is independent and does not import an Auth Center binding. Native Codex providers requiring protocol-converting proxy routes remain explicitly unsupported. Live model requests, real subscription login and interactive Ctrl-C/resume remain manual acceptance items.

## Follow-up verification: default profile and `cc-switch` dispatcher

- 4,346 backend unit tests passed (11 existing tests ignored), with the documented isolated home and cleaned test PATH.
- 81 profile-related backend tests passed. New assertions cover ID-based default selection, persisted default after database reopen, retained session credentials when the default changes, missing-default/deletion errors and shell dispatcher argument/exit behavior.
- 3,481 frontend tests passed in 272 files with the documented 15-second timeout. The UI test checks that selecting a default passes the immutable ID without launching a process or saving provider configuration.
- Both native launcher integration tests passed with the literal profile names `gpt` and `opencode`. They exercise default and named dispatch, concurrent children, arguments/exit codes and byte-for-byte preservation of a pre-existing original Claude settings file containing a dummy original endpoint/key. All credentials and CLIs in these request-path tests are mocked.
- Strict TypeScript, Prettier, rustfmt and the unsigned debug macOS bundle passed. The current app is `src-tauri/target/debug/bundle/macos/CC Switch.app`.

Default selection is local profile metadata, not a global provider switch. No default was selected in the real user's database and no real shell files or original Claude settings were changed by these automated tests. The installed dispatcher is created through the explicit UI install operation. No real subscription request was made.

## Provider verification follow-up (2026-11-01)

The unified diagnostic is documented in [Provider verification](../provider-verification.md). New Rust checks first established a failing response-classification test, then implemented the finite report and request paths. The final full backend unit run passed **4,354 tests, with 11 ignored**, using a temporary `CC_SWITCH_TEST_HOME` and the same cleaned child PATH described above. This includes **8 new verification tests**. The mock upstream asserts the selected key/model and fixed prompt, no generated request during inspection, and unchanged global provider selection. Additional checks cover every tool's saved API schema, account bindings, profile/provider/runtime mismatch, catalog-versus-generation classification, HTTP auth/quota errors, streaming errors and bounded native output.

The full frontend run passed **3,486 tests in 273 files** with `--maxWorkers=2 --minWorkers=1 --testTimeout=15000`. Its initial run found that the edit-dialog test fixture lacked the QueryClient context now used by the diagnostic; the fixture was updated to use the existing application-style test wrapper. After the final card icon styling change, the focused diagnostic/edit/profile suites passed **23 tests**, and the affected provider-card suites passed **35 tests**. No production timeout settings were changed.

Strict TypeScript, rustfmt, Prettier, Clippy with warnings denied and whitespace checking passed. No real provider inference, subscription quota, user login or default credential files were used during verification. Native authentication/output compatibility and provider-specific free-tier restrictions still require manual acceptance with the actual selected profile.

Final security check: refresh-only Gemini credential JSON is rejected before forwarding, so the diagnostic never treats the whole refresh credential as a bearer token. The focused 8 checks and the complete cached unit test binary were rerun afterward: **4,354 passed, 11 ignored**. Both native launcher integration tests passed against the rebuilt CLI. Final Clippy (`--lib -- -D warnings`), strict TypeScript and formatting checks passed. The final renderer and unsigned debug macOS bundle built successfully at `src-tauri/target/debug/bundle/macos/CC Switch.app`. Real subscription/login verification remains manual; this build is not signed/notarized for distribution.
