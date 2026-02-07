# Snippet System & Setup Auto-Execution — Feature Spec

**Status:** Draft
**Phase:** 2
**Depends on:** Phase 1 (workspace lifecycle, PTY, config system)

## Overview

Enhance the existing snippet system (currently a flat `HashMap<String, String>` in `RepoConfig`) into a full-featured snippet manager with structured metadata, global + per-repo storage, a command palette UI, and a trust-gated setup auto-execution flow. The trust store prevents arbitrary code execution from untrusted repository configs.

---

## 1. Snippet Data Model

### 1.1 Rust Structs (`src-tauri/src/snippet.rs`)

```rust
use serde::{Deserialize, Serialize};

/// A single executable snippet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    /// Unique name within its scope (e.g. "test", "lint", "deploy-staging")
    pub name: String,
    /// Shell command to execute
    pub command: String,
    /// Human-readable description shown in the palette
    #[serde(default)]
    pub description: String,
    /// Grouping category for palette filtering
    #[serde(default = "default_category")]
    pub category: SnippetCategory,
    /// Optional keyboard shortcut (e.g. "Ctrl+Shift+T")
    #[serde(default)]
    pub keybinding: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SnippetCategory {
    Setup,
    Build,
    Test,
    Lint,
    Deploy,
    Custom,
}

fn default_category() -> SnippetCategory {
    SnippetCategory::Custom
}

/// Container for snippet storage files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnippetStore {
    #[serde(default = "default_version")]
    pub version: u32,
    pub snippets: Vec<Snippet>,
}

fn default_version() -> u32 {
    1
}
```

### 1.2 TypeScript Interfaces (`src/lib/types.ts`)

```typescript
export type SnippetCategory = "setup" | "build" | "test" | "lint" | "deploy" | "custom";

export interface Snippet {
  name: string;
  command: string;
  description: string;
  category: SnippetCategory;
  keybinding: string | null;
}
```

### 1.3 Storage Layout

| Scope | Path | Purpose |
|---|---|---|
| Global | `~/.ocestrater/snippets.json` | User-wide snippets available in all repos |
| Per-repo | `{repo}/.ocestrater/snippets.json` | Repo-specific snippets, checked into version control |

Both files use the `SnippetStore` schema:

```json
{
  "version": 1,
  "snippets": [
    {
      "name": "test",
      "command": "cargo test",
      "description": "Run all unit tests",
      "category": "test",
      "keybinding": "Ctrl+Shift+T"
    }
  ]
}
```

### 1.4 Resolution Order

When listing snippets for a workspace:

1. Load global snippets from `~/.ocestrater/snippets.json`.
2. Load repo snippets from `{repo_path}/.ocestrater/snippets.json`.
3. Merge: per-repo snippets override global snippets **by name**. If a repo snippet has the same `name` as a global snippet, the repo version wins.
4. Return the merged list sorted by category, then alphabetically by name.

### 1.5 Migration from Existing Format

The current `RepoConfig.snippets: HashMap<String, String>` is a flat name-to-command map. On first load, if the old format exists and no `snippets.json` file is present:

1. Convert each entry to a `Snippet` with `category: "custom"` and empty `description`.
2. Write the new `snippets.json` file.
3. Remove the `snippets` field from `config.json` (or leave it ignored).

This is a one-time migration, handled in `SnippetStore::load_or_migrate`.

---

## 2. Trust Store

### 2.1 Purpose

Repo-level configs can define `setup_script` and snippets that execute arbitrary shell commands. Without trust gating, cloning a malicious repo and opening it in Ocestrater could execute code without user consent. The trust store records which repos the user has explicitly approved.

### 2.2 Rust Structs (`src-tauri/src/trust.rs`)

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustStore {
    #[serde(default = "default_version")]
    pub version: u32,
    /// Global override: if true, all repos are trusted (for advanced users)
    #[serde(default)]
    pub trust_all_repos: bool,
    /// Per-repo trust entries keyed by canonical repo path
    pub repos: HashMap<String, TrustEntry>,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustEntry {
    pub trusted: bool,
    /// ISO 8601 timestamp of when trust was granted
    pub trusted_at: String,
    /// SHA-256 hash of the setup_script content at the time trust was granted.
    /// Used to detect changes that require re-confirmation.
    pub setup_script_hash: Option<String>,
    /// SHA-256 hash of the snippets.json content at the time trust was granted.
    pub snippets_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustStatus {
    /// Repo is trusted and hashes match — safe to auto-execute
    Trusted,
    /// Repo has never been trusted
    Untrusted,
    /// Repo was trusted but setup_script or snippets changed — re-confirm
    Changed {
        changed_files: Vec<String>,
    },
}
```

### 2.3 Storage

**Path:** `~/.ocestrater/trust.json`

```json
{
  "version": 1,
  "trust_all_repos": false,
  "repos": {
    "/Users/dev/my-project": {
      "trusted": true,
      "trusted_at": "2026-02-07T12:00:00Z",
      "setup_script_hash": "a1b2c3d4e5f6...",
      "snippets_hash": "f6e5d4c3b2a1..."
    }
  }
}
```

### 2.4 Trust Check Algorithm

```
check_trust(repo_path):
  1. If trust_all_repos == true → return Trusted
  2. Look up repo_path in trust.repos
  3. If not found → return Untrusted
  4. If entry.trusted == false → return Untrusted
  5. Compute current SHA-256 of:
     - {repo_path}/.ocestrater/config.json → extract setup_script field → hash
     - {repo_path}/.ocestrater/snippets.json → hash entire file
  6. Compare against entry.setup_script_hash and entry.snippets_hash
  7. If any hash mismatches → return Changed { changed_files }
  8. All hashes match → return Trusted
```

### 2.5 TypeScript Interfaces

```typescript
export type TrustStatus =
  | { type: "trusted" }
  | { type: "untrusted" }
  | { type: "changed"; changed_files: string[] };

export interface TrustEntry {
  trusted: boolean;
  trusted_at: string;
  setup_script_hash: string | null;
  snippets_hash: string | null;
}
```

---

## 3. Tauri Commands

All new commands go in `src-tauri/src/commands.rs`. They follow the existing pattern: `State<'_, Mutex<...>>` for shared state, `Result<T, String>` return type.

### 3.1 `list_snippets`

Returns the merged list of global + repo snippets for a given repo.

```rust
#[tauri::command]
pub fn list_snippets(
    config: State<'_, Mutex<ConfigStore>>,
    repo_path: Option<String>,
) -> Result<Vec<Snippet>, String>
```

**Implementation:**
1. Load global snippets from `~/.ocestrater/snippets.json`.
2. If `repo_path` is provided, load repo snippets from `{repo_path}/.ocestrater/snippets.json`.
3. Merge: repo overrides global by `name`.
4. Sort by category, then name.
5. Return merged list.

**IPC binding:**
```typescript
export async function listSnippets(repoPath?: string): Promise<Snippet[]> {
  return invoke("list_snippets", { repoPath });
}
```

### 3.2 `save_snippet`

Saves a snippet to either the global or repo-level store.

```rust
#[tauri::command]
pub fn save_snippet(
    repo_path: Option<String>,
    snippet: Snippet,
) -> Result<(), String>
```

**Implementation:**
1. Determine target file: `repo_path` provided → `{repo_path}/.ocestrater/snippets.json`, else `~/.ocestrater/snippets.json`.
2. Load existing store (or create default).
3. Upsert snippet by `name` (replace if exists, append if new).
4. Write file.

**IPC binding:**
```typescript
export async function saveSnippet(repoPath: string | null, snippet: Snippet): Promise<void> {
  return invoke("save_snippet", { repoPath, snippet });
}
```

### 3.3 `delete_snippet`

Removes a snippet by name from a specific store.

```rust
#[tauri::command]
pub fn delete_snippet(
    repo_path: Option<String>,
    name: String,
) -> Result<(), String>
```

**Implementation:**
1. Determine target file (same as `save_snippet`).
2. Load store.
3. Remove snippet with matching `name`. Error if not found.
4. Write file.

**IPC binding:**
```typescript
export async function deleteSnippet(repoPath: string | null, name: string): Promise<void> {
  return invoke("delete_snippet", { repoPath, name });
}
```

### 3.4 `run_snippet_v2`

Executes a snippet in a workspace's worktree with streaming output.

```rust
#[tauri::command]
pub async fn run_snippet_v2(
    app: AppHandle,
    config: State<'_, Mutex<ConfigStore>>,
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
    name: String,
) -> Result<(), String>
```

**Implementation:**
1. Look up workspace for `worktree_path` and `repo_path`.
2. Resolve snippet: repo-level first, then global (same merge logic as `list_snippets`).
3. If snippet not found → error.
4. Check trust status for the repo. If snippet comes from a repo-level store:
   - If not trusted → return error `"repo not trusted"` (frontend should prompt trust dialog).
   - If trust status is `Changed` → return error `"repo trust stale"`.
5. Spawn `sh -c <command>` as a child process in `worktree_path`.
6. Read stdout/stderr in a background task.
7. Stream output via `app.emit(format!("snippet-output-{workspace_id}"), chunk)`.
8. On exit, emit `app.emit(format!("snippet-complete-{workspace_id}"), exit_code)`.

**Key difference from existing `run_snippet`:** Streaming output via IPC events instead of blocking and returning the full output. This is necessary for long-running snippets (builds, deploys).

**IPC binding:**
```typescript
export async function runSnippetV2(workspaceId: string, name: string): Promise<void> {
  return invoke("run_snippet_v2", { workspaceId, name });
}

export function onSnippetOutput(
  workspaceId: string,
  callback: (data: string) => void,
): Promise<UnlistenFn> {
  return listen<string>(`snippet-output-${workspaceId}`, (event) => {
    callback(event.payload);
  });
}

export function onSnippetComplete(
  workspaceId: string,
  callback: (exitCode: number) => void,
): Promise<UnlistenFn> {
  return listen<number>(`snippet-complete-${workspaceId}`, (event) => {
    callback(event.payload);
  });
}
```

### 3.5 `check_trust`

Returns the trust status for a repo.

```rust
#[tauri::command]
pub fn check_trust(
    repo_path: String,
) -> Result<TrustStatus, String>
```

**Implementation:** Runs the trust check algorithm from section 2.4.

**IPC binding:**
```typescript
export async function checkTrust(repoPath: string): Promise<TrustStatus> {
  return invoke("check_trust", { repoPath });
}
```

### 3.6 `grant_trust`

Marks a repo as trusted, storing current hashes.

```rust
#[tauri::command]
pub fn grant_trust(
    repo_path: String,
) -> Result<(), String>
```

**Implementation:**
1. Load trust store from `~/.ocestrater/trust.json`.
2. Compute SHA-256 hashes of current `setup_script` and `snippets.json`.
3. Insert/update entry: `{ trusted: true, trusted_at: now(), setup_script_hash, snippets_hash }`.
4. Write trust store.

**IPC binding:**
```typescript
export async function grantTrust(repoPath: string): Promise<void> {
  return invoke("grant_trust", { repoPath });
}
```

### 3.7 `revoke_trust`

Removes trust for a repo.

```rust
#[tauri::command]
pub fn revoke_trust(
    repo_path: String,
) -> Result<(), String>
```

**Implementation:**
1. Load trust store.
2. Set `repos[repo_path].trusted = false` (or remove entry entirely).
3. Write trust store.

**IPC binding:**
```typescript
export async function revokeTrust(repoPath: string): Promise<void> {
  return invoke("revoke_trust", { repoPath });
}
```

### 3.8 Command Registration

Add to `lib.rs` `invoke_handler`:

```rust
commands::list_snippets,
commands::save_snippet,
commands::delete_snippet,
commands::run_snippet_v2,
commands::check_trust,
commands::grant_trust,
commands::revoke_trust,
```

---

## 4. Setup Auto-Execution Flow

### 4.1 Trigger

Setup auto-execution runs when `create_workspace` is called and the repo has a `setup_script` configured in its `RepoConfig`.

### 4.2 Flow Diagram

```
create_workspace(repo_path, branch, agent)
  │
  ├── Create git worktree (existing logic)
  │
  ├── Check: does repo have setup_script?
  │     No  → skip to agent spawn
  │     Yes ↓
  │
  ├── check_trust(repo_path)
  │     │
  │     ├── Trusted → run setup_script automatically
  │     │     │
  │     │     ├── Spawn `sh -c <setup_script>` in worktree
  │     │     ├── Stream output via snippet-output-{workspace_id}
  │     │     ├── Wait for completion
  │     │     ├── If exit code != 0 → log warning, continue anyway
  │     │     └── Proceed to agent spawn
  │     │
  │     ├── Untrusted → emit trust-required event
  │     │     │
  │     │     ├── Emit: trust-required-{repo_path}
  │     │     │   payload: { repo_path, script_content, workspace_id }
  │     │     ├── Frontend shows TrustDialog
  │     │     ├── User approves → grant_trust → run setup_script → spawn agent
  │     │     ├── User denies → skip setup, log warning → spawn agent
  │     │     └── (workspace creation does NOT block on trust dialog;
  │     │          agent spawn is deferred until trust is resolved)
  │     │
  │     └── Changed → emit trust-required event (with "changed" context)
  │           │
  │           ├── Emit: trust-required-{repo_path}
  │           │   payload: { repo_path, script_content, workspace_id,
  │           │              changed_files: [...] }
  │           ├── TrustDialog shows diff of what changed
  │           └── Same approve/deny flow as Untrusted
  │
  └── Spawn agent PTY (existing logic)
```

### 4.3 Implementation Changes to `create_workspace`

The current `create_workspace` command runs `setup_script` synchronously via `std::process::Command` with no trust check. This needs to change:

1. After worktree creation, call `check_trust(repo_path)`.
2. If `Trusted`: spawn setup script as a streaming child process (reuse `run_snippet_v2` infrastructure). Wait for completion before spawning agent PTY.
3. If `Untrusted` or `Changed`: emit `trust-required-{repo_path}` IPC event. Return the workspace in a `PendingSetup` state. The frontend handles the trust dialog and calls back:
   - On approve: `grant_trust` then `run_setup_and_start_agent` (new command).
   - On deny: `start_agent_no_setup` (new command, just spawns the PTY).

### 4.4 New Workspace State

Add a `PendingSetup` state to `WorkspaceState`:

```
Creating -> PendingSetup -> Running -> Stopping -> Stopped
                |
                +-> Running (if setup skipped or no setup_script)
```

`PendingSetup` indicates the workspace is created but waiting for trust resolution before the agent starts.

### 4.5 New Supporting Commands

```rust
/// Run setup script then start the agent (called after trust approval)
#[tauri::command]
pub async fn run_setup_and_start_agent(
    app: AppHandle,
    config: State<'_, Mutex<ConfigStore>>,
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    workspace_id: String,
) -> Result<(), String>

/// Skip setup and start the agent directly (called after trust denial)
#[tauri::command]
pub fn start_agent_no_setup(
    config: State<'_, Mutex<ConfigStore>>,
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    workspace_id: String,
) -> Result<(), String>
```

---

## 5. Frontend Components

### 5.1 Component Tree

```
SnippetPalette (Cmd+P overlay)
  +-- SearchInput
  +-- CategoryFilter (horizontal pill bar)
  +-- SnippetList
        +-- SnippetItem (per snippet)

SnippetManager (settings panel)
  +-- ScopeSelector (Global | Repo dropdown)
  +-- SnippetTable
  |     +-- SnippetRow (per snippet, inline editing)
  +-- AddSnippetForm

TrustDialog (modal overlay)
  +-- TrustHeader ("Repository wants to run scripts")
  +-- ScriptPreview (syntax-highlighted readonly code block)
  +-- ChangedFilesList (only for "changed" status)
  +-- ActionButtons (Approve | Deny)
```

### 5.2 SnippetPalette

**File:** `src/components/SnippetPalette.tsx`

```typescript
interface SnippetPaletteProps {
  workspaceId: string;
  repoPath: string;
  onClose: () => void;
}
```

**Trigger:** `Cmd+P` (or `Ctrl+P` on Linux/Windows) while a workspace is active. If the existing command palette is bound to `Cmd+P`, use `Cmd+Shift+P` for snippets (configurable).

**Behavior:**
- Opens as a centered modal overlay with backdrop blur (consistent with existing dialog style).
- Auto-focused search input at the top. Filters snippet list by name, description, and command substring.
- Category pills below search: All (default), Setup, Build, Test, Lint, Deploy, Custom. Click to filter.
- Snippet list shows: name (bold), description (muted), command preview (monospace, truncated), category badge, keybinding hint (right-aligned).
- Items are keyboard-navigable: arrow keys to move selection, Enter to run, Escape to close.
- Running a snippet: calls `runSnippetV2(workspaceId, snippetName)`, closes palette, streams output to workspace terminal.
- Empty state: "No snippets configured. Open Snippet Manager to add some."

### 5.3 SnippetManager

**File:** `src/components/SnippetManager.tsx`

```typescript
interface SnippetManagerProps {
  repoPath: string | null;
}
```

**Trigger:** Accessible from app settings or sidebar context menu on a repo.

**Behavior:**
- Scope selector at top: "Global" or repo name. Determines which store CRUD operations target.
- Table view with columns: Name, Command, Category, Keybinding, Actions (edit/delete).
- Each row is inline-editable: click the edit icon to toggle fields to inputs.
- Add form at bottom: name (required), command (required, multiline textarea for complex commands), description, category dropdown, keybinding input.
- Save calls `saveSnippet(repoPath, snippet)`.
- Delete shows confirmation, then calls `deleteSnippet(repoPath, name)`.
- Visual distinction: repo-level snippets that override a global snippet show an "overrides global" badge.

### 5.4 TrustDialog

**File:** `src/components/TrustDialog.tsx`

```typescript
interface TrustDialogProps {
  repoPath: string;
  scriptContent: string;
  workspaceId: string;
  changedFiles?: string[];
  onApprove: () => void;
  onDeny: () => void;
}
```

**Trigger:** Emitted as `trust-required-{repo_path}` IPC event during workspace creation.

**Behavior:**
- Modal dialog, non-dismissible (must choose approve or deny).
- Header: warning icon + "Repository wants to execute scripts".
- Repo path displayed prominently.
- Script content shown in a read-only code block with syntax highlighting (shell).
- For `Changed` status: additional section listing which files changed since last trust grant, with a diff preview if feasible.
- "Approve" button (primary): calls `grantTrust(repoPath)` then `runSetupAndStartAgent(workspaceId)`.
- "Deny" button (secondary): calls `startAgentNoSetup(workspaceId)`, shows brief toast "Setup skipped".
- "Always trust" checkbox (optional, maps to `trust_all_repos` in config — hidden behind an "Advanced" disclosure).

### 5.5 Snippet Output

Snippet output is streamed to the active workspace's terminal panel via the same `snippet-output-{workspace_id}` events used by `run_snippet_v2`. The terminal already renders PTY output; snippet output appears inline, prefixed with a visual separator:

```
── Running snippet: test ──────────────────────
cargo test
  Compiling my-project v0.1.0
  Running tests...
  test result: ok. 42 passed; 0 failed
── Snippet complete (exit: 0) ─────────────────
```

The prefix/suffix lines are emitted by the backend as part of the output stream.

---

## 6. IPC Events

### 6.1 Event Table

| Event Name | Direction | Payload | Trigger |
|---|---|---|---|
| `snippet-output-{workspace_id}` | Backend -> Frontend | `string` (output chunk) | During snippet execution, streamed |
| `snippet-complete-{workspace_id}` | Backend -> Frontend | `number` (exit code) | Snippet process exits |
| `trust-required-{repo_path}` | Backend -> Frontend | `TrustRequiredPayload` | Workspace creation needs trust approval |

### 6.2 Trust Required Payload

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustRequiredPayload {
    pub repo_path: String,
    pub workspace_id: String,
    pub script_content: String,
    pub changed_files: Vec<String>,
}
```

```typescript
export interface TrustRequiredPayload {
  repo_path: string;
  workspace_id: string;
  script_content: string;
  changed_files: string[];
}

export function onTrustRequired(
  repoPath: string,
  callback: (payload: TrustRequiredPayload) => void,
): Promise<UnlistenFn> {
  return listen<TrustRequiredPayload>(
    `trust-required-${repoPath}`,
    (event) => callback(event.payload),
  );
}
```

### 6.3 Snippet Output Streaming

The backend streams snippet output using the same batching strategy as PTY output (16ms batch interval, 4KB threshold) to prevent event flooding. This reuses the infrastructure from `pty_manager.rs`.

---

## 7. User Journeys

### 7.1 Running a Snippet via Palette

1. User focuses a workspace tab.
2. User presses `Cmd+P` to open SnippetPalette.
3. User types "test" — list filters to matching snippets.
4. User presses Enter on "test" snippet.
5. Palette closes. Terminal shows snippet separator + streamed output.
6. On completion, terminal shows exit status line.

### 7.2 First-Time Workspace in Untrusted Repo

1. User clicks "Create Workspace" on a newly added repo.
2. Backend creates worktree, detects `setup_script` in repo config.
3. Backend calls `check_trust` — returns `Untrusted`.
4. Backend emits `trust-required-{repo_path}` with script content.
5. Frontend shows TrustDialog: "This repository wants to run: `npm install && npm run build`".
6. User reads script, clicks "Approve".
7. Frontend calls `grantTrust(repoPath)` then `runSetupAndStartAgent(workspaceId)`.
8. Setup script runs, output streams to terminal.
9. On completion, agent PTY spawns.

### 7.3 Trusted Repo with Changed Setup Script

1. User updates `setup_script` in repo config (e.g., after pulling new changes).
2. User creates a new workspace.
3. Backend detects trust status: `Changed { changed_files: ["setup_script"] }`.
4. TrustDialog appears: "Setup script has changed since you last approved it."
5. Dialog shows the new script content.
6. User approves — hashes are updated, setup runs.

### 7.4 Managing Snippets

1. User opens SnippetManager from settings.
2. Selects "my-project" repo scope.
3. Sees existing repo snippets + global snippets (global ones marked with a badge).
4. Clicks "Add Snippet". Fills in: name="deploy", command="./scripts/deploy.sh staging", category="deploy".
5. Saves. Snippet appears in list and in the palette.
6. User edits the snippet's keybinding to `Ctrl+Shift+D`.
7. Pressing `Ctrl+Shift+D` now runs the deploy snippet directly (no palette needed).

---

## 8. Performance

### 8.1 Snippet Resolution

Snippet loading and merging is lightweight (two JSON file reads, typically <20 snippets each). No caching needed — resolve on every `list_snippets` call for simplicity and correctness.

### 8.2 Snippet Execution

- Streaming output uses the same batching as PTY output (16ms / 4KB threshold).
- Only one snippet can run per workspace at a time. Attempting to run a second while one is active returns an error.
- Long-running snippets (deploys) should be cancellable: add a `cancel_snippet` command in the future if needed. For v1, the user can stop the workspace to kill child processes.

### 8.3 Trust Check

- Hash computation (SHA-256 of file contents) is fast (<1ms for typical config files).
- Trust check runs once per workspace creation, not on every snippet invocation. Per-snippet trust is inherited from the repo trust status.

---

## 9. Security Considerations

### 9.1 Command Injection

Snippet commands are executed via `sh -c <command>`. The `command` field is stored as-is and passed directly to the shell — this is intentional (snippets are shell commands by design). Security is enforced at the trust layer, not at the execution layer.

### 9.2 Trust Bypass

- The `trust_all_repos` flag is `false` by default and requires explicit user action to enable.
- Global snippets are always trusted (the user created them).
- Repo snippets require the repo to be trusted before execution via `run_snippet_v2`.
- Trust is per-repo, not per-snippet. Trusting a repo trusts all its snippets.

### 9.3 Hash Integrity

- SHA-256 is used for detecting changes, not for cryptographic signing. If the repo config files are modified (e.g., via git pull), the hash mismatch triggers re-confirmation.
- An attacker who can modify `~/.ocestrater/trust.json` can bypass trust checks — but at that point they already have user-level filesystem access.

---

## 10. Files Changed

| File | Change |
|---|---|
| `src-tauri/src/snippet.rs` | **New** — Snippet struct, SnippetStore, load/save/merge logic |
| `src-tauri/src/trust.rs` | **New** — TrustStore, TrustEntry, TrustStatus, check/grant/revoke logic |
| `src-tauri/src/commands.rs` | **Modified** — Add list_snippets, save_snippet, delete_snippet, run_snippet_v2, check_trust, grant_trust, revoke_trust, run_setup_and_start_agent, start_agent_no_setup |
| `src-tauri/src/lib.rs` | **Modified** — Register new commands, initialize TrustStore state |
| `src-tauri/src/config.rs` | **Modified** — Deprecate `snippets: HashMap<String, String>` field on RepoConfig (keep for migration) |
| `src-tauri/src/workspace.rs` | **Modified** — Add `PendingSetup` state to WorkspaceState enum |
| `src/lib/tauri.ts` | **Modified** — Add IPC bindings for all new commands and events |
| `src/lib/types.ts` | **Modified** — Add Snippet, SnippetCategory, TrustStatus, TrustRequiredPayload types |
| `src/components/SnippetPalette.tsx` | **New** — Cmd+P searchable snippet runner |
| `src/components/SnippetManager.tsx` | **New** — CRUD panel for managing snippets |
| `src/components/TrustDialog.tsx` | **New** — Trust confirmation modal |
| `src/App.tsx` | **Modified** — Register Cmd+P keybinding, listen for trust-required events |
