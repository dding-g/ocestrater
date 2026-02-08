# Multi-Agent Parallel Support — Feature Spec

**Status:** Draft
**Phase:** 2
**Depends on:** Phase 1 (complete) — Tauri 2.0 shell, PTY manager, workspace lifecycle

## Overview

Enable running multiple AI agents simultaneously, each in its own workspace with a dedicated terminal, switchable via a tab system. The backend (`PtyManager`) already supports multiple concurrent sessions keyed by workspace ID. This spec covers the frontend state architecture, tab UI, sidebar enhancements, and resource management needed to expose that capability.

---

## 1. Tab System Design

### 1.1 New Component: `TabBar`

Location: `src/components/TabBar.tsx`

A horizontal tab bar rendered directly above `AgentPanel`. Each open workspace gets a tab.

```
+--[repo/branch ● agent]--[repo/branch agent]--[repo/branch agent]--+--------+
|                          AgentPanel (terminal)                      | Review |
```

**Tab anatomy:**
- Repo alias + branch name (truncated with ellipsis at 28 chars)
- Agent badge (small pill, e.g. "claude")
- Status indicator dot (color-coded)
- Close button (x) on hover

### 1.2 Tab States

| State | Visual | Trigger |
|---|---|---|
| `active` | Highlighted background, bold text | User clicks tab |
| `running` | Green pulse dot | PTY alive, workspace status = Running |
| `stopped` | Red dot, dimmed text | PTY exited or workspace stopped |
| `has-new-output` | Blue dot + brief flash | PTY output received while tab is not active |

The `has-new-output` indicator clears when the user switches to that tab.

### 1.3 Tab Interactions

| Action | Behavior |
|---|---|
| Click | Switch active workspace; show that workspace's terminal and review panel |
| Middle-click | Close tab (stop workspace if running, with confirmation) |
| Drag | Reorder tabs (local state only, no persistence) |
| Double-click label | Rename workspace (future; no-op for now) |
| Ctrl/Cmd+W | Close active tab |
| Ctrl/Cmd+Tab | Cycle to next tab |
| Ctrl/Cmd+Shift+Tab | Cycle to previous tab |
| Ctrl/Cmd+1..9 | Jump to tab by position |

### 1.4 Terminal Instance Lifecycle

Each workspace gets its own `Terminal` (xterm.js) instance. Instances are **kept alive** when switching tabs — they are detached from the DOM and reattached, never destroyed, until the tab is explicitly closed.

Strategy:
- Maintain a `Map<workspaceId, { terminal: Terminal, fitAddon: FitAddon, container: HTMLDivElement }>` in a module-level cache (not in SolidJS reactive state, to avoid serialization overhead).
- On tab switch: detach the current terminal's container from the DOM, attach the new one, call `fitAddon.fit()`.
- On tab close: call `terminal.dispose()`, remove from cache.

This avoids re-rendering the terminal (which would lose scrollback and cursor state) and keeps tab switches under the 50ms target.

---

## 2. State Architecture

### 2.1 Current State (Phase 1)

```typescript
// App.tsx — flat signals
const [repos, setRepos] = createSignal<Repo[]>([]);
const [activeWorkspace, setActiveWorkspace] = createSignal<Workspace | null>(null);
```

Single `activeWorkspace` signal. No concept of "open tabs" vs. "all workspaces." Terminal state is local to `AgentPanel` and destroyed on workspace switch.

### 2.2 Proposed State: SolidJS Store

Replace the flat signals with a `createStore` to enable fine-grained reactivity over nested workspace data.

**New file:** `src/store/workspace-store.ts`

```typescript
import { createStore } from "solid-js/store";

interface TerminalBuffer {
  /** Whether this tab has unread output since last focus */
  hasNewOutput: boolean;
  /** Accumulated output size in bytes (for memory tracking) */
  outputBytes: number;
}

interface WorkspaceTab {
  id: string;
  repoPath: string;
  repoAlias: string;
  branch: string;
  agent: string;
  status: "idle" | "running" | "stopped";
  terminalBuffer: TerminalBuffer;
}

interface WorkspaceState {
  /** All open tabs, ordered by tab bar position */
  tabs: WorkspaceTab[];
  /** Currently visible workspace ID, or null if none open */
  activeId: string | null;
  /** All known repos (sidebar data) */
  repos: Repo[];
}

const [state, setState] = createStore<WorkspaceState>({
  tabs: [],
  activeId: null,
  repos: [],
});
```

### 2.3 Store Actions

Expose a clean action API (not raw setState) so components stay decoupled from store shape:

```typescript
// Actions — each is a plain function that calls setState internally

function openWorkspace(ws: Workspace): void
  // Adds to tabs[] if not already present, sets activeId

function closeWorkspace(id: string): void
  // Removes from tabs[], disposes terminal, selects adjacent tab

function setActiveWorkspace(id: string): void
  // Updates activeId, clears hasNewOutput for that tab

function reorderTabs(fromIndex: number, toIndex: number): void
  // Reorders tabs[] array

function updateWorkspaceStatus(id: string, status: WorkspaceTab["status"]): void
  // Updates status field for a specific tab

function markNewOutput(id: string): void
  // Sets hasNewOutput = true if tab is not active

function clearNewOutput(id: string): void
  // Sets hasNewOutput = false
```

### 2.4 Terminal State Preservation

Terminal content (scrollback buffer, cursor position, selection) is managed entirely by xterm.js instances, **not** by the SolidJS store. The store only tracks metadata (status, hasNewOutput, outputBytes).

The terminal cache lives outside the reactive system:

```typescript
// src/lib/terminal-cache.ts

const cache = new Map<string, {
  terminal: Terminal;
  fitAddon: FitAddon;
  element: HTMLDivElement;
  unlisten: UnlistenFn[];  // IPC event listeners
}>();
```

This separation is intentional:
- SolidJS store drives UI reactivity (tab bar highlights, sidebar badges).
- Terminal cache is imperative DOM management (attach/detach elements).
- No double-bookkeeping of terminal content.

### 2.5 IPC Event Wiring

Each workspace's PTY output is already namespaced by the Rust backend (`pty-output-{workspace_id}`). When a workspace tab is opened:

1. Call `onPtyOutput(workspaceId, callback)` to subscribe.
2. In the callback: write data to the cached terminal, increment `outputBytes`, call `markNewOutput(id)` if tab is not active.
3. Call `onPtyExit(workspaceId, callback)` to handle agent exit.
4. Store the `UnlistenFn` handles in the terminal cache for cleanup.

On tab close, call all stored `UnlistenFn` handles, then `terminal.dispose()`.

---

## 3. Sidebar Updates

### 3.1 Active Workspace Highlighting

Currently, clicking a workspace in the sidebar calls `onSelectWorkspace`. This will now also call `openWorkspace` (which adds a tab if needed and sets it active).

The sidebar item `.active` class should match `state.activeId`, same as today.

### 3.2 "New Workspace" Button Per Repo

Add a `+` icon button inside each expanded repo group header (next to the workspace count badge). Clicking it opens a workspace creation dialog/popover with:
- Branch selector (text input with autocomplete from `git branch -r`)
- Agent selector (dropdown from `getAgents()`)
- "Create" button

```
my-app  [3]  [+]
  ▾
    ● main / claude
    ● feat-x / codex
```

### 3.3 Agent Status Pulse/Badge

Replace the static status dot with an animated indicator:

| Status | Visual |
|---|---|
| `running` | Green dot with CSS `pulse` animation (subtle scale 1.0 -> 1.4 -> 1.0, 2s loop) |
| `stopped` | Static red dot |
| `idle` | Static gray dot |
| `has-new-output` | Blue dot with single flash (one-shot animation, not looping) |

### 3.4 Workspace Context Menu

Right-click on a workspace item shows a native-style context menu:

| Menu Item | Action | Enabled When |
|---|---|---|
| Stop Agent | `stopWorkspace(id)` | status = running |
| Restart Agent | stop then recreate | status = running or stopped |
| Remove Workspace | `removeWorkspace(id)` | status = stopped |
| Copy Worktree Path | copy to clipboard | always |

Implementation: Use a simple SolidJS `<Show>` positioned absolutely on right-click coordinates. No external menu library needed.

---

## 4. Resource Management

### 4.1 Max Concurrent PTY Sessions

**Default limit:** 8 concurrent PTY sessions.
**Configurable:** Via `~/.ocestrater/config.json` under `defaults.max_concurrent_agents`.

Enforcement points:
- **Frontend:** Disable "New Workspace" / "Create" buttons when at limit. Show tooltip: "Maximum concurrent agents reached (8). Stop an existing agent to create a new one."
- **Backend:** `PtyManager::spawn` checks `self.sessions.len() >= max` and returns an error.

Add to Rust `PtyManager`:
```rust
pub struct PtyManager {
    sessions: HashMap<String, PtySession>,
    app_handle: AppHandle,
    max_sessions: usize,  // new field
}
```

### 4.2 Memory Budget Per Terminal

**Scrollback limit:** 10,000 lines per terminal (already set in current `AgentPanel`). This translates to roughly 5-15MB per terminal depending on line length.

When a terminal's `outputBytes` exceeds 20MB, emit a warning badge on the tab. At 50MB, auto-trim the scrollback to 5,000 lines and log a warning.

Track via the `outputBytes` counter in the store, incremented in the PTY output callback.

### 4.3 Graceful Degradation

| Condition | Behavior |
|---|---|
| Hit max PTY sessions | Block new workspace creation with clear error |
| Terminal memory warning (20MB) | Yellow badge on tab, log warning |
| Terminal memory critical (50MB) | Auto-trim scrollback, notify user |
| PTY reader thread panics | Mark workspace as stopped, show error in terminal, allow retry |
| IPC event backpressure | Batching already handles this (16ms interval); if queue exceeds 64KB, drop oldest batch and log |

---

## 5. Performance Targets

### 5.1 Tab Switch: < 50ms

How this is achieved:
- xterm.js instances are never destroyed/recreated on switch — only DOM attach/detach.
- `fitAddon.fit()` is the only call on switch (fast: reads container dimensions, calls `terminal.resize`).
- SolidJS store update (`activeId`) triggers minimal DOM diffing (tab bar highlight change, header text swap).
- No IPC calls on tab switch — output listeners remain subscribed in background.

**Measurement:** Use `performance.now()` around the `setActiveWorkspace` action in dev mode.

### 5.2 4 Concurrent Agents: < 200MB Total Memory

Memory budget breakdown (per agent):
- xterm.js Terminal instance: ~15MB (10K scrollback, average line length)
- PTY process (Rust side): ~5MB (child process overhead + reader buffer)
- IPC event overhead: ~2MB (batched output buffer)
- **Per-agent total: ~22MB**
- **4 agents: ~88MB** — well within the 200MB target

App shell overhead (SolidJS, Tauri webview, Rust runtime): ~60MB baseline.

**Total projected: ~150MB for 4 agents** with headroom.

### 5.3 IPC: Batched Per-Workspace Events, No Cross-Talk

Already implemented correctly in `pty_manager.rs`:
- Each PTY reader thread emits to `pty-output-{workspace_id}` — namespaced, no cross-talk.
- 16ms batch interval (~60fps) prevents event flooding.
- Frontend listeners are per-workspace (`onPtyOutput(workspaceId, callback)`).

No changes needed to the IPC layer. The batching constants (`BATCH_INTERVAL_MS`, 4KB threshold) are adequate for 4-8 concurrent sessions.

---

## 6. Component Hierarchy (Updated)

```
App
├── Sidebar
│   ├── RepoGroup (per repo)
│   │   ├── RepoHeader [+]
│   │   └── WorkspaceItem (per workspace)
│   │       └── ContextMenu (on right-click)
│   └── AddRepoButton
├── MainPanel (new wrapper)
│   ├── TabBar
│   │   └── Tab (per open workspace)
│   └── AgentPanel
│       ├── AgentHeader
│       ├── TerminalContainer (swappable)
│       └── InputBar
└── ReviewPanel
```

### Key Changes from Phase 1:
- `AgentPanel` no longer creates/owns terminal instances. It provides a mount point; `terminal-cache.ts` manages instances.
- `TabBar` is a new sibling above `AgentPanel`, both wrapped in `MainPanel`.
- `App.tsx` consumes from the workspace store instead of local signals.

---

## 7. Migration Path

### Step 1: Create workspace store (`src/store/workspace-store.ts`)
- Define types, store, and actions.
- Export actions and reactive selectors.

### Step 2: Create terminal cache (`src/lib/terminal-cache.ts`)
- Map-based cache for xterm.js instances.
- Attach/detach helpers.
- IPC listener management per workspace.

### Step 3: Build `TabBar` component
- Render from `state.tabs`.
- Wire click/middle-click/keyboard handlers to store actions.

### Step 4: Refactor `AgentPanel`
- Remove internal terminal creation.
- Accept a container element from the cache.
- Keep input bar and header; header reads from active workspace in store.

### Step 5: Update `Sidebar`
- Add per-repo "+" button.
- Add context menu.
- Wire to store actions instead of prop callbacks.

### Step 6: Update `App.tsx`
- Remove `activeWorkspace` and `repos` signals.
- Import from workspace store.
- Pass store-derived values to child components.

### Step 7: Add resource limits
- `max_sessions` field on `PtyManager`.
- `max_concurrent_agents` in `GlobalConfig.defaults`.
- Frontend limit enforcement in the create workspace flow.

---

## 8. Files Changed

| File | Change |
|---|---|
| `src/store/workspace-store.ts` | **New** — central state store |
| `src/lib/terminal-cache.ts` | **New** — xterm.js instance cache |
| `src/components/TabBar.tsx` | **New** — tab bar component |
| `src/components/MainPanel.tsx` | **New** — wrapper for TabBar + AgentPanel |
| `src/App.tsx` | **Modified** — consume store, remove local signals |
| `src/components/AgentPanel.tsx` | **Modified** — delegate terminal to cache, keep input bar |
| `src/components/Sidebar.tsx` | **Modified** — add per-repo "+", context menu, store integration |
| `src/components/ReviewPanel.tsx` | **Modified** — read active workspace from store |
| `src-tauri/src/pty_manager.rs` | **Modified** — add `max_sessions` field and check |
| `src-tauri/src/config.rs` | **Modified** — add `max_concurrent_agents` to `Defaults` |
| `src-tauri/src/commands.rs` | **Modified** — pass max_sessions config to PtyManager |
