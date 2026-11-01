# Claude launch profiles

This feature adds **Launch profiles** to the Claude Code, Codex and OpenCode provider pages. It is separate from the project snapshot switcher.

## Default profile and CCS-style commands

In **Launch profiles**, click **Use as default** on the profile you want. The selected row shows a **Default** badge and the dialog summary shows the current choice. Click **Install / repair cc-switch**, then open a new terminal:

```sh
cc-switch                    # the default profile selected in the UI
cc-switch gpt                # the profile named gpt
cc-switch opencode           # the profile named opencode
cc-switch gpt --resume ID     # arguments forwarded to that profile's CLI
cc-switch -- "review code"   # pass a prompt to the default profile explicitly
```

`gpt` and `opencode` are configured profile names, not hard-coded account selectors. Set their CLI application and provider in the UI. The two named commands can run concurrently. Names matching native CLI binaries (`claude`, `codex`, `opencode`) work under the `cc-switch <name>` namespace; installation does not replace those native binaries. Existing separate profile launchers remain supported.

The default is stored by immutable profile ID in the local profile database. Renaming a profile retains the choice. Changing the default affects future launches only: it does not switch the global provider, rewrite original Claude configuration, invalidate current routed sessions or copy credentials. Restarting the application retains the choice. Removing the default leaves no default; plain `cc-switch` then reports an error and never chooses another account automatically. Unknown named profiles also fail instead of falling back to the default.

Choosing or running a profile **does not overwrite the original Anthropic Claude user directory**. Global provider switching elsewhere in CC Switch retains its existing behavior and may update original live configuration; these are separate operations.

The installed `~/.local/bin/cc-switch` wrapper dispatches to the native binary before desktop initialization. Double-clicking the app bundle still opens the desktop UI. The existing `cc-switch profile run <name>` form remains supported. No existing `ccs` installation is modified.

## Configure and install commands in the UI

On the Claude Code, Codex or OpenCode page, add or edit a provider. The **Profile launcher** section lets you enter a command (for example `claude-gpt`, `codex-work`, or `opencode-work`) and an optional initial model. Leave **Install this command when saving the provider** selected, then save. Leaving the command empty preserves existing provider workflows.

Provider saving and launcher installation are separate steps. If the provider saves but installation fails, the UI reports that explicitly; open **Launch profiles** to retry. This avoids silently undoing provider configuration or claiming a command was installed when it was not.

The **Launch profiles** dialog is available on all three pages. It supports selecting the CLI, managing independent profiles, **Save & install command**, installed/not-installed status, **Install / repair command**, **Copy command**, and **Open terminal**. A failed installation keeps the saved profile available for retry. Claude proxy profiles start the existing server-only local routing operation when their command is installed.

Commands are installed in `~/.local/bin`. Installation appends one managed PATH block to `.zshrc`, `.bashrc` and `.bash_profile`, preserving their existing contents. Open a **new terminal** afterward; existing terminal environments are not modified. No shell function needs to be written manually. Fish, custom ZDOTDIR and existing aliases/functions may need manual PATH/conflict resolution. Run `type -a <command>` if a different command takes precedence.

Generated scripts contain the installed native executable path and immutable profile ID, never provider keys or OAuth tokens. Renaming and reinstalling removes older aliases owned by the same profile. Unrelated files and commands on PATH are rejected rather than overwritten. Removing a profile removes its current owned launcher while keeping its configuration files. Any leftover old launcher references a removed ID and fails instead of selecting a new profile with a reused name. Repair launchers if you move the app bundle.

### Runtime behavior

- **Claude Code:** existing official login isolation and authenticated, provider-specific proxy routing remain in use. For ChatGPT through Claude, choose a Claude-side Codex OAuth provider with an explicit Auth Center account binding.
- **Codex:** launches `codex` with a private `CODEX_HOME`. API providers reuse the existing Codex override builder and pass the primary API key through the child environment. Their compatibility checks examine this profile's config, not the global config. Official Codex profiles use a separate native Codex login; existing Auth Center account credentials/bindings are not imported. Sign in to the intended account inside that profile. Providers requiring cross-format routing are explicitly rejected for native Codex profiles for now; they are not redirected through a global route.
- **OpenCode:** launches `opencode` with a private config directory and XDG config/data/cache/state paths. The selected provider declaration, enabled-provider list and initial model are supplied through inline configuration and `--model`. Both stored V1 and V2 provider formats are preserved using the repository's existing format resolver. Models default to the first configured model when none is supplied. Provider-specific OpenCode OAuth login, if needed, must be set up in the isolated profile. No global auth file is copied.

Project configuration and organization policies still apply according to each CLI. Profile isolation is not an OS sandbox or a restriction on explicit user-supplied CLI overrides. Commands preserve arbitrary working directories, arguments, interactive stdio and Unix exit behavior.

## Setup

1. Install the desired CLI and confirm `claude`, `codex` or `opencode` is available in the terminal PATH.
2. Add providers using the existing provider editor. For GPT, use the existing Claude-side Codex OAuth preset, sign in through Auth Center and bind a specific account to the provider. Do not choose a mutable default account.
3. Open Claude launch profiles. Create `Claude Pro` with launcher `claude-pro` and the official Anthropic provider. Leave the directory blank to allocate a private directory. Sign in separately when the profile starts; existing credentials are never imported automatically.
4. Create `GPT` with launcher `claude-gpt` and your Claude-side Codex OAuth provider. Local routing is selected automatically. Use **Start profile routing** in this dialog and keep the desktop app running. This reuses the existing server-only start operation and leaves default Claude live configuration untouched.
5. Use **Open terminal**, or run the native executable directly:

```sh
/path/to/cc-switch profile run claude-pro
/path/to/cc-switch profile run claude-gpt
/path/to/cc-switch profile run claude-gpt --resume SESSION_ID
```

The command retains the invoking working directory and forwards arguments exactly. GUI terminal launch starts in Terminal's default directory; use the CLI for a specific project.

For the requested short names, add these shell functions to your shell configuration after installing the app in /Applications (adjust the path for a different install location):

```sh
claude-pro() { "/Applications/CC Switch.app/Contents/MacOS/cc-switch" profile run claude-pro "$@"; }
claude-gpt() { "/Applications/CC Switch.app/Contents/MacOS/cc-switch" profile run claude-gpt "$@"; }
```

No account secret belongs in these functions. The plain Save action stores profile metadata; use Save & install command or Install / repair command to install it. The shell functions above are now an optional alternative.

## Isolation and lifecycle

Profiles allocate independent user directories; the default ~/.claude and its login remain untouched. Existing directory selection is a no-copy import: use a dedicated directory you own, not default Claude storage or an Auth Center account directory. CC Switch creates the directory but does not write settings, MCP, Skills, plugin files or credentials. Configure these separately inside the profile. Duplication copies metadata and allocates a new directory; it does not copy files or login. Removal keeps all user files.

Project .claude settings, project MCP and organization policies still apply. Review `/status` for loaded settings. Launch rejects pre-existing endpoint/authentication overrides in profile settings and project settings through the repository root. It leaves those files untouched. Avoid adding overrides or manually editing profile authentication while sessions are running. This is configuration isolation, not a sandbox against local processes or project configuration. Profile session browsing is not integrated into the existing session manager yet; use Claude's own resume commands within the profile.

The initial model is optional and can be changed through Claude Code. It is not permanently pinned. Global UI provider switches do not select the provider for authenticated profile-route requests. Editing a routed profile or its provider invalidates existing session credentials; restart the profile to accept changes. Profile credentials persist in the child environment across proxy restarts and their digests remain in SQLite. Restarts must use the same listener address and port. Deleted/missing providers and authentication failures must never fall back to a different account.

Official profiles authenticate with their own Claude Code login; selecting an official provider with an Auth Center binding does not copy that account's credentials. Authenticate the intended account inside the new profile. Compatibility and subscription access remain controlled by each upstream; no quota or policy bypass is provided.

## Troubleshooting and security

- Unknown profile: check the launcher name in the dialog.
- Cannot launch claude: install Claude Code and fix PATH. GUI Terminal inherits the login shell's PATH.
- Proxy unavailable: start local routing in the desktop app. Profile launch checks loopback health and fails if unavailable. A stopped proxy results in connection errors.
- Invalid/revoked session: provider/profile changed or was removed; review and relaunch.
- OAuth error: verify the explicit account binding and reauthenticate through existing Auth Center controls.
- Endpoint check success only proves reachability; authentication/model compatibility can still fail.
- Relative, overlapping, default-storage and symlink directory paths are rejected. On macOS use the actual path rather than a symlink spelling (for example /private/tmp rather than /tmp).

The proxy uses a random per-launch capability passed in the child environment; only its SHA-256 digest is stored in launch session rows. Upstream credentials remain in existing provider/OAuth storage. Do not publish process environments or database backups. Same-user processes can access environments and files; profiles do not create OS user isolation. New directories use 0700 on Unix. Existing file permissions are retained. Existing legacy proxy routes retain their existing authentication boundary.

## Migration and rollback

Back up the database before upgrading. New tables are created idempotently; existing providers and project snapshots are retained. No default login, Skills or MCP migration occurs. Existing dedicated directories can be selected without copying files. Optional shared Skills/MCP synchronization is deferred; any future implementation must explicitly select non-sensitive items.

To roll back, stop profile sessions and run the previous app version. Retain profile directories. After backing up, the new launch_profile_sessions and launch_profiles tables may be dropped; do not drop the existing profiles table. Old app versions do not understand new launch profiles. Additive table creation without a schema-version increment requires maintainer review before release.

## Verify a provider or private login

Use **Verify provider** on a provider card, in its edit screen, or on a saved launch profile. **Check connection / login** does not generate text. It distinguishes saved credentials from verified access. Choose the profile to check its private login; the default CLI account is separate. To verify model access, accept the quota notice and use **Send test request**. The result shows endpoint, authentication and model response separately. See [Provider verification](../provider-verification.md) for supported protocols and limitations.
