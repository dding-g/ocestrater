# Git Diff & Review Flow - Feature Spec

**Status:** Draft
**Phase:** 2
**Depends on:** Phase 1 (workspace lifecycle, PTY, config system)

---

## 1. Data Types

### 1.1 Rust Structs (`src-tauri/src/git_ops.rs`)

```rust
use serde::{Deserialize, Serialize};

/// Status of a single file in the diff
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// Summary of a changed file (used in file tree listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,   // Only set for Renamed/Copied
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
}

/// A single line within a hunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    /// "add" | "delete" | "context"
    pub kind: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

/// A contiguous region of changes within a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub header: String,            // e.g. "@@ -10,7 +10,9 @@ fn main()"
    pub lines: Vec<DiffLine>,
}

/// Complete diff for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub binary: bool,
    pub hunks: Vec<DiffHunk>,
    pub additions: u32,
    pub deletions: u32,
}

/// Overall status of a workspace's worktree compared to its base branch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeStatus {
    pub workspace_id: String,
    pub base_branch: String,
    pub head_sha: String,
    pub base_sha: String,
    pub ahead: u32,                // commits ahead of base
    pub behind: u32,               // commits behind base
    pub files_changed: u32,
    pub total_additions: u32,
    pub total_deletions: u32,
    pub files: Vec<FileChange>,
    pub has_conflicts: bool,
    pub conflict_files: Vec<String>,
}

/// Result of a merge operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub success: bool,
    pub merge_sha: Option<String>,
    pub conflicts: Vec<String>,     // file paths with conflicts
    pub message: String,
}

/// Strategy for merging worktree changes back to parent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    Merge,          // git merge
    Squash,         // squash all commits into one
    Rebase,         // rebase onto base branch
}

/// Which version of a file to retrieve
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileVersion {
    Base,           // base branch version
    Working,        // current worktree version
}
```

### 1.2 TypeScript Interfaces (`src/lib/tauri.ts`)

```typescript
// ── Git Review Types ──

export type FileStatus = "added" | "modified" | "deleted" | "renamed" | "copied";

export interface FileChange {
  path: string;
  old_path: string | null;
  status: FileStatus;
  additions: number;
  deletions: number;
  binary: boolean;
}

export interface DiffLine {
  kind: "add" | "delete" | "context";
  old_lineno: number | null;
  new_lineno: number | null;
  content: string;
}

export interface DiffHunk {
  old_start: number;
  old_count: number;
  new_start: number;
  new_count: number;
  header: string;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  old_path: string | null;
  status: FileStatus;
  binary: boolean;
  hunks: DiffHunk[];
  additions: number;
  deletions: number;
}

export interface WorktreeStatus {
  workspace_id: string;
  base_branch: string;
  head_sha: string;
  base_sha: string;
  ahead: number;
  behind: number;
  files_changed: number;
  total_additions: number;
  total_deletions: number;
  files: FileChange[];
  has_conflicts: boolean;
  conflict_files: string[];
}

export interface MergeResult {
  success: boolean;
  merge_sha: string | null;
  conflicts: string[];
  message: string;
}

export type MergeStrategy = "merge" | "squash" | "rebase";
export type FileVersion = "base" | "working";
```

---

## 2. Tauri Commands

All new commands go in `src-tauri/src/commands.rs`. They follow the existing pattern: `State<'_, Mutex<WorkspaceManager>>` for workspace lookup, `Result<T, String>` return type.

### 2.1 `get_worktree_status`

Returns a high-level summary of workspace changes vs. base branch.

```rust
#[tauri::command]
pub fn get_worktree_status(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
) -> Result<WorktreeStatus, String>
```

**Implementation:**
1. Look up workspace from `ws_mgr` to get `worktree_path` and `repo_path`.
2. Resolve base branch via `git merge-base HEAD <base>`.
3. Run `git diff --stat --numstat <base>...HEAD` in the worktree for file-level stats.
4. Run `git rev-list --count --left-right <base>...HEAD` for ahead/behind.
5. Parse output into `WorktreeStatus`.

**IPC binding:**
```typescript
export async function getWorktreeStatus(workspaceId: string): Promise<WorktreeStatus> {
  return invoke("get_worktree_status", { workspaceId });
}
```

### 2.2 `get_diff`

Returns full diff content with hunks for all changed files (or a subset).

```rust
#[tauri::command]
pub fn get_diff(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
    paths: Option<Vec<String>>,   // filter to specific files; None = all
) -> Result<Vec<FileDiff>, String>
```

**Implementation:**
1. Look up workspace for `worktree_path`.
2. Resolve base SHA via `git merge-base`.
3. Run `git diff <base>...HEAD --unified=3` (optionally with `-- path1 path2`).
4. Parse unified diff output into `Vec<FileDiff>` with hunks and line metadata.
5. For large diffs (>500 hunks total), truncate and set a flag so the frontend can request per-file.

**IPC binding:**
```typescript
export async function getDiff(
  workspaceId: string,
  paths?: string[],
): Promise<FileDiff[]> {
  return invoke("get_diff", { workspaceId, paths });
}
```

### 2.3 `get_file_content`

Returns the raw content of a file at either the base or working version.

```rust
#[tauri::command]
pub fn get_file_content(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
    path: String,
    version: FileVersion,
) -> Result<String, String>
```

**Implementation:**
- `FileVersion::Working` -> read file directly from `worktree_path/path`.
- `FileVersion::Base` -> `git show <base_sha>:<path>` in the worktree.

**IPC binding:**
```typescript
export async function getFileContent(
  workspaceId: string,
  path: string,
  version: FileVersion,
): Promise<string> {
  return invoke("get_file_content", { workspaceId, path, version });
}
```

### 2.4 `merge_workspace`

Merges worktree changes back into the base branch.

```rust
#[tauri::command]
pub fn merge_workspace(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    workspace_id: String,
    strategy: MergeStrategy,
    commit_message: Option<String>,
) -> Result<MergeResult, String>
```

**Implementation:**
1. Validate workspace state is `Stopped` (agent must be stopped before merge).
2. Based on `strategy`:
   - **Merge:** In the main repo, run `git merge <worktree-branch>`.
   - **Squash:** In the main repo, run `git merge --squash <worktree-branch>` then `git commit`.
   - **Rebase:** In the worktree, run `git rebase <base-branch>`, then in main repo do a fast-forward merge.
3. If conflicts arise, abort the merge and return conflict file list in `MergeResult`.
4. On success, return the new merge SHA.

**IPC binding:**
```typescript
export async function mergeWorkspace(
  workspaceId: string,
  strategy: MergeStrategy,
  commitMessage?: string,
): Promise<MergeResult> {
  return invoke("merge_workspace", { workspaceId, strategy, commitMessage });
}
```

### 2.5 `discard_workspace`

Discards all changes and cleans up the worktree (wraps existing `remove_workspace`).

```rust
#[tauri::command]
pub fn discard_workspace(
    ws_mgr: State<'_, Mutex<WorkspaceManager>>,
    pty_mgr: State<'_, Mutex<PtyManager>>,
    workspace_id: String,
) -> Result<(), String>
```

**Implementation:**
1. Stop PTY if running.
2. Delete the worktree branch: `git branch -D <worktree-branch>`.
3. Remove the worktree directory via existing `WorkspaceManager::remove`.

**IPC binding:**
```typescript
export async function discardWorkspace(workspaceId: string): Promise<void> {
  return invoke("discard_workspace", { workspaceId });
}
```

### 2.6 Command Registration

Add to `lib.rs` `invoke_handler`:

```rust
commands::get_worktree_status,
commands::get_diff,
commands::get_file_content,
commands::merge_workspace,
commands::discard_workspace,
```

---

## 3. IPC Contract

### 3.1 Events (Backend -> Frontend)

Follow the existing pattern in `pty_manager.rs` where events are namespaced per workspace.

| Event Name | Payload | Trigger |
|---|---|---|
| `diff-ready-{workspace_id}` | `WorktreeStatus` | Emitted after agent PTY exits and diff is computed |
| `merge-progress-{workspace_id}` | `{ step: string, progress: number }` | During merge operations |
| `merge-complete-{workspace_id}` | `MergeResult` | After merge finishes |

**Registration pattern (frontend):**
```typescript
export function onDiffReady(
  workspaceId: string,
  callback: (status: WorktreeStatus) => void,
): Promise<UnlistenFn> {
  return listen<WorktreeStatus>(`diff-ready-${workspaceId}`, (event) => {
    callback(event.payload);
  });
}

export function onMergeComplete(
  workspaceId: string,
  callback: (result: MergeResult) => void,
): Promise<UnlistenFn> {
  return listen<MergeResult>(`merge-complete-${workspaceId}`, (event) => {
    callback(event.payload);
  });
}
```

### 3.2 Auto-diff on Agent Exit

When a PTY exits (`pty-exit-{workspace_id}`), the backend should automatically compute and cache the diff, then emit `diff-ready-{workspace_id}`. This ensures the review panel populates without an extra round-trip.

**Implementation:** Add a hook in `PtyManager` or register a listener in `lib.rs` setup:
```rust
// In lib.rs setup, after PtyManager init:
let ws_clone = ws_mgr_state.clone();
app.listen("pty-exit-*", move |event| {
    // Extract workspace_id, compute diff, emit diff-ready event
});
```

Alternatively, compute the diff lazily on first `get_worktree_status` call and cache it.

### 3.3 Error Handling

All commands return `Result<T, String>`. The frontend wraps calls with:
```typescript
try {
  const status = await getWorktreeStatus(workspaceId);
} catch (err) {
  // err is the String from Rust Result::Err
  showError(String(err));
}
```

Error categories:
- **Workspace not found** - invalid `workspace_id`
- **Git error** - underlying git command failed (non-zero exit, parse error)
- **Invalid state** - e.g., trying to merge a running workspace
- **Conflict** - merge resulted in conflicts (returned in `MergeResult`, not thrown)

---

## 4. Frontend Components

### 4.1 Component Tree

```
ReviewPanel (existing, enhanced)
  +-- ReviewHeader
  |     +-- TabBar (Files | Diff | Terminal)
  |     +-- StatusBadge (ahead/behind, file count)
  +-- FileTree (when tab = "Files")
  |     +-- FileTreeGroup (per directory, collapsible)
  |           +-- FileTreeItem (per file)
  +-- DiffViewer (when tab = "Diff")
  |     +-- DiffFileHeader (file path, stats, collapse toggle)
  |     +-- DiffHunkView (per hunk)
  |           +-- DiffLineView (per line, with line numbers and +/- coloring)
  +-- TerminalPane (when tab = "Terminal", existing placeholder)
  +-- ReviewActions (sticky footer)
        +-- MergeButton (primary action)
        +-- StrategySelector (merge | squash | rebase dropdown)
        +-- DiscardButton (danger action with confirmation)
```

### 4.2 FileTree Component

**File:** `src/components/review/FileTree.tsx`

```typescript
interface FileTreeProps {
  files: FileChange[];
  selectedPath: string | null;
  onSelectFile: (path: string) => void;
}
```

**Behavior:**
- Groups files by directory (split on `/`). Top-level files appear ungrouped.
- Directories are collapsible. Default: expanded if <= 20 files total, collapsed per-directory if > 20.
- Each `FileTreeItem` shows: status icon (A/M/D/R/C colored), file name (bold), relative dir path (muted), +/- stats.
- Click selects file and switches to Diff tab filtered to that file.
- Keyboard: arrow keys navigate, Enter selects, left/right collapse/expand directories.

### 4.3 DiffViewer Component

**File:** `src/components/review/DiffViewer.tsx`

```typescript
interface DiffViewerProps {
  diffs: FileDiff[];
  selectedPath: string | null;
  onFileVisible: (path: string) => void;  // for scroll sync with file tree
}
```

**Behavior:**
- Renders all file diffs in a scrollable list (inline/unified format).
- Each file section has a sticky header with file path, stats, and collapse toggle.
- Hunks render with:
  - Left gutter: old line number (muted for additions)
  - Right gutter: new line number (muted for deletions)
  - Line content with syntax highlighting prefix: `+` green bg, `-` red bg, ` ` no bg
- Binary files show "Binary file changed" placeholder.
- When `selectedPath` is set, auto-scroll to that file's section.

**Line number rendering:**
```
old | new | content
 10 |  10 |  unchanged line
 11 |     | -removed line
    |  11 | +added line
 12 |  12 |  unchanged line
```

### 4.4 ReviewActions Component

**File:** `src/components/review/ReviewActions.tsx`

```typescript
interface ReviewActionsProps {
  workspace: Workspace;
  status: WorktreeStatus | null;
  onMerge: (strategy: MergeStrategy, message?: string) => void;
  onDiscard: () => void;
  merging: boolean;
}
```

**Behavior:**
- Merge button: primary CTA, disabled while workspace is `Running` or while `merging`.
- Strategy selector: dropdown defaulting to "squash". Options: Merge, Squash, Rebase.
- Commit message input: shown for Squash strategy, pre-filled with branch name summary.
- Discard button: secondary danger button. On click, shows confirmation dialog ("Discard all changes? This cannot be undone.").
- During merge: button shows spinner + "Merging..." text, all actions disabled.
- After merge success: show success banner, auto-cleanup workspace after 3s or on dismiss.
- On merge conflict: show conflict file list, disable merge, show "Conflicts detected" warning.

### 4.5 Data Flow

```
Agent PTY exits
  -> Backend emits diff-ready-{id} with WorktreeStatus
  -> ReviewPanel receives event, stores in signal
  -> FileTree renders from status.files
  -> User clicks file
  -> Frontend calls getDiff(workspaceId, [selectedPath])
  -> DiffViewer renders hunks
  -> User clicks Merge
  -> Frontend calls mergeWorkspace(id, strategy, message)
  -> ReviewActions shows loading state
  -> Backend emits merge-complete-{id}
  -> On success: cleanup workspace, remove from sidebar
  -> On conflict: show conflict list, abort
```

**State management (signals in ReviewPanel):**
```typescript
const [status, setStatus] = createSignal<WorktreeStatus | null>(null);
const [diffs, setDiffs] = createSignal<FileDiff[]>([]);
const [selectedFile, setSelectedFile] = createSignal<string | null>(null);
const [merging, setMerging] = createSignal(false);
const [mergeResult, setMergeResult] = createSignal<MergeResult | null>(null);
const [diffLoading, setDiffLoading] = createSignal(false);
```

---

## 5. Review Flow - User Journey

### 5.1 Agent Completes Work

1. Agent PTY exits (user stopped it, or agent finished naturally).
2. Workspace state transitions: `Running` -> `Stopped`.
3. Backend automatically computes diff against base branch.
4. Backend emits `diff-ready-{workspace_id}` with `WorktreeStatus`.
5. ReviewPanel's "Changes" tab badge updates with file count.

### 5.2 Browsing Changes

1. User sees ReviewPanel populate with file list (was "No changes yet" before).
2. **Files tab** shows hierarchical tree: grouped by directory, sorted alphabetically.
3. User expands directories, sees per-file status icons and +/- counts.
4. User clicks a file -> tab switches to "Diff", diff loads for that file.
5. **Diff tab** shows unified diff with colored lines, line numbers, hunk headers.
6. User can scroll through all files or use file tree to jump to specific files.
7. User can click on a file path in the diff header to view full file content (opens `get_file_content` with "working" version).

### 5.3 Approve & Merge

1. User selects merge strategy from dropdown (default: Squash).
2. For Squash: user edits commit message (pre-filled: "feat: <branch-name>").
3. User clicks "Merge" button.
4. Button enters loading state. ReviewActions shows progress.
5. Backend runs merge in the main repo directory.
6. **On success:**
   - `merge-complete` event fires with `MergeResult { success: true, merge_sha }`.
   - Success banner appears: "Changes merged successfully (sha: abc1234)".
   - After 3 seconds (or user click), workspace is cleaned up via `remove_workspace`.
   - Workspace removed from sidebar.
7. **On conflict:**
   - `MergeResult { success: false, conflicts: [...] }` returned.
   - Merge is auto-aborted on the backend (`git merge --abort`).
   - Warning banner: "Merge conflicts detected in N files".
   - Conflict files highlighted in red in the file tree.
   - User must resolve manually (outside Ocestrater) or discard.

### 5.4 Reject & Discard

1. User clicks "Discard" button.
2. Confirmation dialog: "Discard all changes in <branch>? This deletes the worktree and branch. This cannot be undone."
3. On confirm: `discard_workspace` is called.
4. Backend deletes worktree branch, removes worktree directory.
5. Workspace removed from sidebar.

### 5.5 Re-review (Agent Restart)

1. If the user wants the agent to do more work before merging, they can restart the agent from the sidebar.
2. This transitions workspace back to `Running`, spawns a new PTY in the same worktree.
3. The previous diff state is cleared. New diff is computed when the agent exits again.

---

## 6. Performance

### 6.1 Diff Computation

- Run git commands asynchronously using `tokio::process::Command` (or `std::thread::spawn` + channel) to avoid blocking the main thread.
- Cache the parsed diff result per workspace. Invalidate on: agent restart, manual refresh.
- For repos with >1000 changed files: return `WorktreeStatus` with file list immediately, defer full hunk parsing to per-file `get_diff` calls.

### 6.2 Large Diff Rendering

- **Virtual scrolling:** DiffViewer uses a virtual list (e.g., `@tanstack/virtual` adapted for Solid, or a custom implementation using `IntersectionObserver`).
- Each file section is measured and virtualized as a unit. Only visible file sections render their hunks.
- Target: handle 500+ changed files and 50,000+ diff lines without frame drops.
- **Lazy hunk loading:** File sections start collapsed if total diff > 200 files. Expanding a file triggers `get_diff(workspaceId, [path])` if hunks weren't pre-loaded.

### 6.3 Incremental Loading Strategy

```
Initial load (on diff-ready event):
  1. Receive WorktreeStatus with file list + stats (fast, no hunk data)
  2. FileTree renders immediately from file list
  3. Request full diff for first 20 visible files in parallel
  4. As user scrolls, request diffs for newly visible files (debounced 100ms)

Per-file diff request:
  - get_diff(workspaceId, [path]) returns FileDiff for that single file
  - Cached in a Map<string, FileDiff> signal
  - Subsequent visits use cached data
```

### 6.4 Memory Management

- DiffLine content strings can be large. For files with >5000 lines of diff, show only first 1000 lines with "Show more" button.
- When user navigates away from ReviewPanel (selects different workspace), dispose diff data to free memory.
- Binary files: never load content, show placeholder only.

### 6.5 File Content Viewer

- `get_file_content` streams large files: for files >1MB, the backend truncates at 1MB and returns a `truncated: true` flag.
- Frontend renders file content with line numbers in a simple `<pre>` block (no syntax highlighting in v1, can add later via `shiki` or `highlight.js`).

---

## 7. New File Structure

```
src-tauri/src/
  git_ops.rs          # NEW: Git diff parsing, merge logic, all structs
  commands.rs         # MODIFIED: Add new command functions
  lib.rs              # MODIFIED: Register new commands

src/
  lib/tauri.ts        # MODIFIED: Add new IPC bindings
  components/
    ReviewPanel.tsx   # MODIFIED: Enhanced with real data flow
    review/
      FileTree.tsx    # NEW
      DiffViewer.tsx  # NEW
      ReviewActions.tsx # NEW
      diff-styles.css # NEW: Shared diff coloring styles
```

---

## 8. Workspace State Machine Update

The existing `WorkspaceState` enum in `workspace.rs` needs a new state to represent the review phase:

```
Creating -> Running -> Stopping -> Stopped -> Reviewing -> Cleaning
                                      |          |
                                      |          +-> Merging -> Cleaning
                                      |
                                      +-> (restart) -> Running
```

New states:
- **Reviewing** - agent stopped, diff computed, user is reviewing changes
- **Merging** - merge operation in progress

The transition from `Stopped` to `Reviewing` happens automatically when `diff-ready` is emitted. `Reviewing` to `Merging` happens on `merge_workspace` call. `Merging` to `Cleaning` happens on merge success.

This is optional for v1 -- the flow works with existing states, but these states improve UI accuracy (e.g., showing "Reviewing" badge instead of "Stopped").
