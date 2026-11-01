# Multi-profile Claude Code

Profiles launch concurrent Claude Code processes using independent configuration directories and stable Claude provider IDs. Official login goes directly to Anthropic; API and Codex OAuth providers use the existing format-converting proxy. Subscription compatibility remains subject to upstream policy; the feature grants no additional access.

CRUD, duplication, launcher aliases, optional initial model, terminal launch, and provider validation are required. Global switching must not affect profile requests. Provider changes, deletion, invalid credentials and proxy outages must fail closed. Existing project snapshot profiles remain separate.

Acceptance: AT-01 concurrent different providers; AT-02 global switching isolation; AT-03 restart persistence; AT-04 missing providers; AT-05 credential isolation; AT-06 arguments/exit status; AT-07 legacy behavior; AT-08 file preservation; AT-09 same-profile consistency; AT-10 failure without fallback. Automated validation must use mocks.

| Requirement | Contract |
| --- | --- |
| FR-01 | CRUD and metadata-only duplication; unique ID/launcher/directory; stable Claude provider ID; routing mode, optional initial model and timestamps. |
| FR-02 | Independent user and credential directory; preserve existing bytes; no automatic credential/MCP/Skills copying. Existing dedicated directories may be adopted explicitly. Shared project and organization settings remain applicable. |
| FR-03 | Authenticated profile requests resolve their own provider, bypass global switching/failover, survive proxy restart, and reject deleted/edited associations. Initial models remain user-changeable. |
| FR-04 | Native pre-GUI `profile run` command; exact argument forwarding, cwd/terminal/exit preservation on Unix; shell functions contain no credentials. macOS terminal action is provided. |
| FR-05 | Focused management dialog with provider/directory/model fields, endpoint reachability check, server-only routing start, terminal action, duplication and file-preserving removal. |
| FR-06 | Independent configuration initially; optional explicit non-sensitive synchronization is deferred. Existing global Skills/MCP management remains intact. |
| FR-07 | Additive database records; no mandatory migration of existing settings, sessions or login; global routes and project snapshot profiles retain their behavior. |

Security/reliability: private new Unix directories; explicit OAuth account binding; hashed local session credentials; parameterized SQL and validated launcher names; atomic profile/session mutations; no raw tokens in scripts/URLs/logs. Existing upstream authentication and policy enforcement are reused. Profile directories do not provide an OS sandbox; local processes, enterprise policy and manual post-launch settings changes remain part of the boundary.

Follow-up scope: provider forms now optionally create and install a command while saving. Profiles select Claude Code, native Codex or OpenCode; old records default to Claude. Command installation/status/copy/repair are available in the UI. Shell PATH setup targets new zsh/bash terminals and preserves existing content. This does not promise that unsupported native Codex proxy formats, external shell aliases, fish/custom shell initialization or live account login are automatically configured.

Default-profile follow-up: the UI must select a default by stable ID. `cc-switch` launches that profile, while `cc-switch <name>` launches an explicitly named profile. Changing the default must not switch the global provider, write original Anthropic Claude files or redirect running sessions. Missing/deleted defaults and unknown names must fail closed. Native executable names such as `opencode` are valid namespaced profile names without overwriting the native CLI.
