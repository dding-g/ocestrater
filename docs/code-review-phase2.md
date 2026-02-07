# Phase 2 Code Review - Ocestrater

## Summary

Ocestrater Phase 2 is a solid Tauri-based AI agent orchestrator with a Rust backend (PTY management, git worktree operations, config) and a SolidJS frontend (workspace tabs, terminal rendering, diff viewer). The architecture is clean with good module separation, proper IPC boundaries, and consistent type contracts between Rust and TypeScript. The codebase has several issues that range from critical (potential deadlock patterns, memory/resource leaks, security concerns with command execution) to minor (unused dependencies, hardcoded dark-mode colors). The diff parser and git operations are well-implemented but need edge-case hardening for production use.

---

## Critical Issues (Must Fix)

### C1. Deadlock risk from nested mutex acquisitions in `discard_workspace`

**File:** `src-tauri/src/commands.rs:358-387`

`discard_workspace` acquires `ws_mgr` lock, drops it, performs git operations, then re-acquires `ws_mgr` lock. Between the drop and re-acquire, `ws.remove()` may fail because the workspace state might still be `Running` (the PTY was killed but `stop()` was never called to update the workspace state to `Stopped`). The `remove()` method at `workspace.rs:115` checks `if ws.state == WorkspaceState::Running` and returns an error.

**Impact:** `discard_workspace` will always fail for running workspaces because it kills the PTY but never transitions the workspace state from `Running` to `Stopped`.

**Fix:** Call `ws.stop()` after killing the PTY, before calling `ws.remove()`.

### C2. `closeWorkspace` race condition in workspace-store.ts

**File:** `src/store/workspace-store.ts:62-78`

When a tab is closed, `closeWorkspace` filters the tab out of `state.tabs`, then reads `state.tabs` again to find the next tab. However, due to SolidJS store reactivity, the `remaining` variable at line 70 reads from the already-filtered store, meaning `remaining.length` is correct, but `Math.min(idx, remaining.length - 1)` can produce an out-of-bounds index if `idx` pointed to the last element.

Specifically: if you close the last tab in the list, `idx` equals the old length - 1, and `remaining.length - 1` equals old length - 2. `Math.min(old_length - 1, old_length - 2)` = `old_length - 2`, which is correct. This is actually fine on closer inspection -- the logic is correct but confusing. **Downgraded to minor.**

### C3. Rust edition "2024" in Cargo.toml is incorrect

**File:** `src-tauri/Cargo.toml:4`

`edition = "2024"` -- Rust 2024 edition requires nightly/very recent Rust toolchain. This should likely be `"2021"` for stable compatibility. If this is intentional for cutting-edge features, it should be documented.

**Impact:** Build failure on stable Rust toolchains.

### C4. `MAX_SCROLLBACK` constant defined but never used

**File:** `src-tauri/src/pty_manager.rs:9`

`MAX_SCROLLBACK` is defined but never referenced. More importantly, there is no scrollback management in the PTY reader thread -- the batched output is emitted to the frontend but nothing limits how much data accumulates server-side. The terminal cache on the frontend (`scrollback: 10000` in xterm config) handles it client-side, but the Rust side has no protection against a runaway agent producing unlimited output.

**Impact:** Potential memory growth if IPC emission falls behind PTY output rate.

---

## Important Issues (Should Fix)

### I1. Setup script and snippet execution is vulnerable to injection

**File:** `src-tauri/src/commands.rs:123, 232`

Both `setup_script` and `run_snippet` pass user-configured strings directly to `sh -c`. While these scripts come from config files that the user controls, the pattern `Command::new("sh").args(["-c", &script])` means any malicious config content runs with full shell access.

**Recommendation:** Document that config files are trusted, or add validation/sandboxing. At minimum, log what script is about to run so users can audit.

### I2. PTY sessions are never cleaned up when process exits naturally

**File:** `src-tauri/src/pty_manager.rs:96-131`

When the reader thread detects the PTY process has exited (line 117 `Err(_) => break`), it sets `alive = false` and emits `pty-exit`, but the session entry remains in the `HashMap`. Over time, dead sessions accumulate and count toward `max_sessions` limit, preventing new workspace creation.

**Recommendation:** Either have the reader thread signal cleanup via a channel, or check `alive` status in `spawn()` and clean up dead sessions first.

### I3. `onPtyOutput`/`onPtyExit` listeners have async registration race

**File:** `src/lib/terminal-cache.ts:54-63`

The event listeners are registered via `.then()` after `onPtyOutput()` returns a Promise. If PTY output arrives before the Promise resolves and the `UnlistenFn` is pushed to `unlistenHandles`, the listener might not be properly tracked for cleanup.

More critically, the `unlistenHandles` array is captured by reference and pushed to asynchronously. The `CachedTerminal` entry is set in the cache before listeners are fully registered. If `destroyTerminal` is called immediately after `createTerminal`, the unlisten functions won't exist yet.

**Recommendation:** Use `await` or register listeners synchronously before returning the cache entry.

### I4. Diff viewer does not handle large diffs efficiently

**File:** `src/components/review/DiffViewer.tsx`

All diff lines are rendered as DOM elements. For large diffs (1000+ files, 10000+ lines), this will cause severe performance issues. There is no virtualization.

**Recommendation:** For Phase 2 this is acceptable, but add virtualization (e.g., `@tanstack/virtual`) before production use.

### I5. `notify` and `thiserror` crate dependencies are unused

**File:** `src-tauri/Cargo.toml:23-24`

`notify = "7"` and `thiserror = "2"` are listed as dependencies but never used in any Rust source file. `tokio` with `features = ["full"]` is also included but the entire backend is synchronous.

**Recommendation:** Remove unused dependencies to reduce compile time and binary size.

### I6. `WorkspaceInfo` field naming inconsistency between Rust and TypeScript

**File:** `src/lib/tauri.ts:42-50` vs `src-tauri/src/workspace.rs:16-25`

The Rust struct uses `snake_case` field names (`repo_path`, `repo_alias`, `worktree_path`), and serde serializes them as snake_case. The TypeScript interface correctly mirrors this with `repo_path`, `repo_alias`, etc. However, `App.tsx:8-15` defines a `Workspace` interface with `camelCase` fields (`repoPath`, `repoAlias`), and the store uses `camelCase` throughout.

This means there must be a manual mapping somewhere between the Tauri IPC response and the store, but it's not visible in the code. The `Sidebar.tsx` imports `Workspace` from `App.tsx` and uses `ws.repoPath`, but the IPC returns `repo_path`. This is either:
1. A bug where data doesn't flow correctly, or
2. There's mapping code not shown in the reviewed files.

**Impact:** Potentially broken data flow from backend to frontend store.

### I7. `handleAddRepo` in Sidebar has dead import attempt

**File:** `src/components/Sidebar.tsx:134-145`

```typescript
const { open } = await import("@tauri-apps/plugin-shell");
```

This attempts to dynamically import `open` from the shell plugin, but never uses it and falls through to the `catch` block with a `window.prompt` fallback. This should either be implemented properly with the Tauri dialog plugin or removed.

### I8. `reviewWidth` signal in App.tsx is created but ReviewPanel is not conditionally resizable

**File:** `src/App.tsx:25`

The `reviewWidth` signal and `setReviewWidth` are passed to `ReviewPanel`, but the `ReviewPanel` component always renders when `activeWorkspace()` is truthy. There is no way to hide/toggle the review panel. The review panel's resize handle is separate from the sidebar's, but they share the same CSS class `.resize-handle`.

---

## Minor Issues (Nice to Fix)

### M1. Duplicate `resolve_base_branch` implementation

Both `workspace.rs:159-173` and `git_ops.rs:130-146` contain identical `detect_base_branch`/`resolve_base_branch` logic. This should be consolidated into `git_ops.rs`.

### M2. `statusColor` function in Sidebar.tsx is defined but functionally redundant

**File:** `src/components/Sidebar.tsx:55-64`

`statusColor()` returns CSS variable strings, and the result is applied via inline style. The `statusDotClass()` function already classifies status. The inline style could be replaced with CSS classes for consistency.

### M3. Hardcoded dark theme colors in DiffViewer

**File:** `src/components/review/DiffViewer.tsx:238-241`

```css
.dv-line-add { background: #2d4a2d; }
.dv-line-del { background: #4a2d2d; }
```

These are hardcoded hex colors that won't adapt to a light theme. Should use CSS variables like the rest of the codebase.

### M4. `createEffect` unused import in DiffViewer.tsx

**File:** `src/components/review/DiffViewer.tsx:1`

`onMount` is imported but never used.

### M5. Missing `button` element semantics

Several interactive `<span>` elements (e.g., `.tab-close`, `.repo-add-ws`) should be `<button>` elements for accessibility.

### M6. No `Untracked` file status handling

The git diff parser handles A/M/D/R/C statuses but doesn't handle `U` (unmerged/untracked) which can appear in some git diff contexts.

---

## Performance Assessment

**Overall: Good for Phase 2, needs attention before production.**

1. **IPC Batching (16ms):** The 16ms batch interval in `pty_manager.rs` is appropriate for ~60fps updates. The 4096-byte threshold provides a good fallback for burst output. The `BufReader::lines()` approach however loses raw PTY escape sequences since it splits on newlines -- this may cause rendering issues with agents that output partial ANSI sequences.

2. **Terminal Cache:** The cache pattern in `terminal-cache.ts` is well-designed. Terminals are created once and attached/detached from DOM as tabs switch, avoiding expensive re-initialization. The `scrollback: 10000` limit prevents unlimited memory growth on the client side.

3. **Diff Viewer:** For typical code review scenarios (< 100 files), the current non-virtualized approach is fine. For repositories with 1000+ changed files, it will be slow. The `buildTree()` function runs on every render (via `createMemo`), which is correct for memoization.

4. **Lock Contention:** The Rust backend uses `std::sync::Mutex` for all shared state. Since Tauri commands run on a thread pool, contention is possible under heavy concurrent workspace operations. The `create_workspace` command notably holds the config lock while reading config, drops it, then acquires ws_mgr and pty_mgr locks sequentially -- this is correct and avoids nested locks.

5. **Git Operations:** All git commands are synchronous `Command::new("git")` calls. For large repositories, `compute_diff` and `compute_status` could block the Tauri command thread for seconds. Consider moving these to async tasks.

---

## Security Assessment

**Overall: Acceptable for a local desktop app, needs hardening for distribution.**

1. **Command Execution:** The `setup_script` and `run_snippet` features execute arbitrary shell commands from config files. Since these configs are user-authored and local, this is acceptable. However, if config files are ever synced or shared (e.g., `.ocestrater/config.json` in a repo), this becomes a supply-chain vector.

2. **PTY Spawning:** Agent commands come from the global config (`~/.ocestrater/config.json`). The `AgentAdapter` adds flags like `--dangerously-skip-permissions` for Claude, which is documented behavior but should be called out in user-facing docs.

3. **Git Operations:** All git commands use argument arrays (not string concatenation), which prevents command injection. Path arguments come from controlled sources (workspace manager state).

4. **Config Parsing:** Malformed JSON in config files is handled gracefully with `unwrap_or_else(|_| Self::default_...)` fallbacks. This is correct.

5. **IPC:** Tauri's invoke system provides type-safe IPC. The `listen` API for events is per-workspace-id scoped, which prevents cross-workspace data leakage.

---

## Architecture Assessment

**Overall: Well-structured with clean module boundaries.**

### Rust Backend

- **Module Separation:** Clean separation between `config`, `workspace`, `pty_manager`, `git_ops`, `agent`, and `commands`. The `commands` module serves as the IPC boundary layer.
- **No Circular Dependencies:** Each module has clear, one-directional dependencies. `commands` depends on all others; no other module depends on `commands`.
- **Error Handling:** Consistent `Result<T, String>` pattern throughout. While `String` errors lose type information compared to `thiserror` enums, it's pragmatic for IPC where errors are serialized to strings anyway.
- **State Management:** All state is behind `Mutex<T>` managed by Tauri's `app.manage()`. Lock ordering is consistent (config -> ws_mgr -> pty_mgr), which prevents deadlocks.

### Frontend

- **Store Design:** The SolidJS store in `workspace-store.ts` is well-structured with clear actions and selectors. Fine-grained reactivity works well with the tab/workspace model.
- **Component Hierarchy:** `App > Sidebar | (TabBar + AgentPanel) | ReviewPanel` is a clean layout. Components are appropriately sized and focused.
- **Type Safety:** TypeScript types in `types.ts` mirror Rust types accurately. No `any` types detected in the reviewed files.
- **CSS:** Component-scoped styles via `<style>` tags in JSX. CSS variables in `global.css` provide consistent theming. The only issue is hardcoded colors in the diff viewer.

### IPC Contract

- The naming mismatch between Rust (`snake_case` serde) and TypeScript (mixed `snake_case` in types.ts / `camelCase` in App.tsx) needs resolution. Either use `#[serde(rename_all = "camelCase")]` on Rust structs or consistently use `snake_case` in the frontend.
