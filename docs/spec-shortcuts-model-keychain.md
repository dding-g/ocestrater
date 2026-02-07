# Keyboard Shortcuts, Model Selector & macOS Keychain — Feature Spec

**Status:** Draft
**Phase:** 3
**Depends on:** Phase 2 (complete) — Multi-agent tabs, workspace store, terminal cache

## Overview

Three interconnected features that improve power-user workflow and security:

1. **Keyboard Shortcut System** — configurable, JSON-based shortcut registry with conflict detection.
2. **Agent Model Selector** — per-workspace model switching with PTY restart.
3. **macOS Keychain Integration** — secure API key storage via the system keychain, injected into agent PTY sessions.

---

## 1. Keyboard Shortcut System

### 1.1 Shortcut Configuration

**File:** `~/.ocestrater/shortcuts.json`

A flat JSON map of action names to key bindings. The app loads this file at startup and watches for changes via the existing `notify` crate watcher.

```json
{
  "version": 1,
  "shortcuts": {
    "workspace.new":        "Cmd+N",
    "workspace.close":      "Cmd+W",
    "tab.1":                "Cmd+1",
    "tab.2":                "Cmd+2",
    "tab.3":                "Cmd+3",
    "tab.4":                "Cmd+4",
    "tab.5":                "Cmd+5",
    "tab.6":                "Cmd+6",
    "tab.7":                "Cmd+7",
    "tab.8":                "Cmd+8",
    "tab.9":                "Cmd+9",
    "tab.next":             "Cmd+Tab",
    "tab.prev":             "Cmd+Shift+Tab",
    "palette.snippets":     "Cmd+P",
    "message.send":         "Cmd+Enter",
    "palette.command":      "Cmd+K",
    "settings.open":        "Cmd+,",
    "agent.restart":        "Cmd+Shift+R"
  }
}
```

**Key format:** Modifier keys use platform-normalized names: `Cmd`, `Ctrl`, `Shift`, `Alt`. Multiple modifiers are joined with `+`. The final segment is the key name (case-insensitive), matching `KeyboardEvent.key` values.

**Validation rules:**
- Each binding must have at least one modifier (`Cmd`, `Ctrl`, `Alt`).
- Reserved OS shortcuts (`Cmd+Q`, `Cmd+H`, `Cmd+M`) are rejected.
- Duplicate bindings across different actions trigger a warning logged to the console and surfaced in the Settings modal.

### 1.2 Default Shortcuts

| Action | Shortcut | Description |
|---|---|---|
| `workspace.new` | `Cmd+N` | Open new workspace dialog |
| `workspace.close` | `Cmd+W` | Close active tab |
| `tab.1` ... `tab.9` | `Cmd+1` ... `Cmd+9` | Switch to tab N |
| `tab.next` | `Cmd+Tab` | Cycle to next tab |
| `tab.prev` | `Cmd+Shift+Tab` | Cycle to previous tab |
| `palette.snippets` | `Cmd+P` | Open snippet palette |
| `message.send` | `Cmd+Enter` | Send message in input bar |
| `palette.command` | `Cmd+K` | Open command palette (future) |
| `settings.open` | `Cmd+,` | Open settings modal (future) |
| `agent.restart` | `Cmd+Shift+R` | Restart active agent |

### 1.3 Architecture

```
KeyboardEvent (window)
       │
       ▼
 ShortcutHandler (global listener)
       │
       ├─ normalize(event) → "Cmd+Shift+R"
       │
       ├─ shortcutMap.get("Cmd+Shift+R") → "agent.restart"
       │
       └─ actionDispatch("agent.restart") → handler function
```

**Components:**

1. **ShortcutParser** (`src/lib/shortcut-parser.ts`) — parses the JSON config into a `Map<string, string>` (keybinding string to action name). Normalizes modifier order to a canonical form: `Ctrl+Alt+Shift+Cmd+<Key>` for deterministic lookup.

2. **ShortcutHandler** (`src/components/ShortcutHandler.tsx`) — a SolidJS component (renders nothing) that attaches a single `keydown` listener to `window` on mount. On each event:
   - Builds the canonical keybinding string from the event.
   - Performs O(1) `Map.get()` lookup.
   - If matched, calls `e.preventDefault()` and dispatches the action.
   - If no match, the event propagates normally.

3. **ActionRegistry** (`src/lib/action-registry.ts`) — maps action names to handler functions. Components register their handlers on mount and unregister on cleanup.

```typescript
// src/lib/action-registry.ts
const handlers = new Map<string, () => void>();

export function registerAction(name: string, handler: () => void): void {
  handlers.set(name, handler);
}

export function unregisterAction(name: string): void {
  handlers.delete(name);
}

export function dispatchAction(name: string): boolean {
  const handler = handlers.get(name);
  if (handler) {
    handler();
    return true;
  }
  return false;
}
```

### 1.4 Migration from Existing Shortcuts

`TabBar.tsx` currently has a hardcoded `handleKeyboard` listener (lines 43-82) that handles `Cmd+Tab`, `Cmd+Shift+Tab`, `Cmd+1-9`, and `Cmd+W`. This listener will be **removed** and replaced by:

1. `ShortcutHandler` component mounted in `App.tsx` (single global listener).
2. `TabBar` registers its actions (`tab.next`, `tab.prev`, `tab.1`...`tab.9`, `workspace.close`) via `registerAction` on mount.
3. `AgentPanel` registers `message.send` and its input-bar-specific handling.

This eliminates scattered `addEventListener` calls and centralizes all shortcut logic.

### 1.5 Hot Reload

The Rust backend watches `~/.ocestrater/shortcuts.json` using the existing `notify` dependency. On file change:

1. Backend re-reads and validates the file.
2. Emits `shortcuts-updated` IPC event with the new map.
3. Frontend `ShortcutHandler` replaces its internal `Map` — no component remount needed.

### 1.6 Conflict Detection

When loading shortcuts (startup or hot-reload):

1. Build a reverse map: `keybinding → action[]`.
2. Any keybinding mapping to 2+ actions is a conflict.
3. Conflicts are reported as:
   - Console warning: `"Shortcut conflict: Cmd+P is bound to both palette.snippets and palette.command"`.
   - `conflicts` field in the `shortcuts-updated` IPC payload, displayed in the Settings modal.
4. On conflict, the **first** action in file order wins.

---

## 2. Agent Model Selector

### 2.1 Model Configuration

Extend `AgentConfig` in `src-tauri/src/config.rs` with model support:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub models: Vec<String>,           // new
    #[serde(default)]
    pub default_model: Option<String>, // new
    #[serde(default)]
    pub model_flag: Option<String>,    // new — e.g. "--model"
}
```

**Default config (`~/.ocestrater/config.json`):**

```json
{
  "agents": {
    "claude": {
      "command": "claude",
      "args": [],
      "models": ["opus", "sonnet", "haiku"],
      "default_model": "sonnet",
      "model_flag": "--model"
    },
    "codex": {
      "command": "codex",
      "args": [],
      "models": ["o3", "o4-mini", "gpt-4.1"],
      "default_model": "o4-mini",
      "model_flag": "--model"
    },
    "gemini": {
      "command": "gemini",
      "args": [],
      "models": ["gemini-2.5-pro", "gemini-2.5-flash"],
      "default_model": "gemini-2.5-flash",
      "model_flag": "--model"
    }
  }
}
```

The `model_flag` field allows agents with different CLI conventions. If `model_flag` is null/absent, model selection is not supported for that agent.

### 2.2 Per-Workspace Model State

Extend `WorkspaceTab` in `src/store/workspace-store.ts`:

```typescript
interface WorkspaceTab {
  id: string;
  repoPath: string;
  repoAlias: string;
  branch: string;
  agent: string;
  model: string | null;  // new — currently selected model, null = agent default
  status: "idle" | "running" | "stopped";
  terminalBuffer: TerminalBuffer;
}
```

New store action:

```typescript
export function setWorkspaceModel(id: string, model: string): void {
  setState("tabs", (t) => t.id === id, "model", model);
}
```

### 2.3 Model Switching Flow

Switching a model mid-session requires a PTY restart because agent CLIs do not support hot-swapping models.

**Sequence:**

```
User clicks model in dropdown
       │
       ▼
Frontend: setWorkspaceModel(id, "opus")
       │
       ▼
Frontend: invoke("switch_agent_model", { workspaceId, model: "opus" })
       │
       ▼
Backend: PtyManager::kill(workspace_id)
       │
       ▼
Backend: Rebuild command with --model opus flag
       │
       ▼
Backend: PtyManager::spawn(workspace_id, adapter, working_dir)
       │
       ▼
Frontend: receives pty-exit then new pty-output events
       │
       ▼
Terminal: clears scrollback, shows new session
```

**`AgentAdapter::build_command` update:**

```rust
pub fn build_command(&self, model: Option<&str>) -> (String, Vec<String>) {
    let mut args = self.config.args.clone();

    // Inject model flag if specified
    if let (Some(model), Some(flag)) = (model, &self.config.model_flag) {
        args.push(flag.clone());
        args.push(model.to_string());
    }

    // Per-agent adaptations (existing logic)
    match self.name.as_str() {
        "claude" => { /* ... existing ... */ }
        "codex" => { /* ... existing ... */ }
        "gemini" => { /* ... existing ... */ }
        _ => {}
    }

    (self.config.command.clone(), args)
}
```

### 2.4 Frontend: ModelSelector Component

**Location:** `src/components/ModelSelector.tsx`

Rendered inside `AgentPanel`'s `.agent-header`, replacing the static agent badge.

```
┌─────────────────────────────────────────────────────────┐
│  my-app / main   [claude ▾ sonnet]   ● running          │
└─────────────────────────────────────────────────────────┘
```

**Behavior:**
- Displays current model as a clickable badge (e.g., `sonnet`).
- On click, opens a dropdown listing available models from the agent's `models` array.
- Selected model is highlighted with a checkmark.
- Selecting a different model triggers the switch flow (section 2.3).
- Dropdown closes on selection, click-outside, or `Escape`.
- During PTY restart, the badge shows a spinner and the dropdown is disabled.
- If the agent has no `models` defined (empty array), the badge shows just the agent name with no dropdown.

**Styling:** The dropdown uses the same design language as existing components — `var(--bg-secondary)` background, `var(--border)` borders, `var(--accent)` for the selected item.

### 2.5 Model Switch Performance

Target: < 2 seconds from click to new session ready.

Breakdown:
- `PtyManager::kill`: ~50ms (signal + drop writer).
- PTY teardown / process exit: ~200ms.
- `PtyManager::spawn`: ~100ms (pty allocation + process start).
- Agent CLI initialization: ~1-1.5s (varies by agent).
- Total: ~1.5-2s typical.

During the transition, the terminal shows a "Switching to model..." message written by the frontend (not via PTY) to provide immediate feedback.

---

## 3. macOS Keychain Integration

### 3.1 Rust Crate

Add `security-framework` to `src-tauri/Cargo.toml`:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
security-framework = "3"
```

This provides native macOS Keychain access without shelling out to the `security` CLI.

### 3.2 Keychain Service

**Service name:** `com.ocestrater.secrets`
**Account name convention:** The key name itself (e.g., `ANTHROPIC_API_KEY`).

All items are stored as generic passwords in the default keychain.

### 3.3 Backend Module

**New file:** `src-tauri/src/keychain.rs`

```rust
use security_framework::passwords::{
    set_generic_password, get_generic_password, delete_generic_password,
};

const SERVICE: &str = "com.ocestrater.secrets";

pub fn get_secret(key: &str) -> Result<String, String> {
    get_generic_password(SERVICE, key)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|e| format!("keychain read error: {e}"))
}

pub fn set_secret(key: &str, value: &str) -> Result<(), String> {
    set_generic_password(SERVICE, key, value.as_bytes())
        .map_err(|e| format!("keychain write error: {e}"))
}

pub fn delete_secret(key: &str) -> Result<(), String> {
    delete_generic_password(SERVICE, key)
        .map_err(|e| format!("keychain delete error: {e}"))
}
```

**Note on `list_secret_keys`:** The macOS Keychain API does not provide a straightforward "list all items for a service" method without using the lower-level `SecItemCopyMatching` API. To keep this simple, we maintain a separate index:

- **Key index file:** `~/.ocestrater/secret-keys.json` — a JSON array of key names (no values).
- `set_secret` adds the key name to the index.
- `delete_secret` removes the key name from the index.
- `list_secret_keys` reads the index file.

```json
["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GOOGLE_API_KEY"]
```

This avoids complex Keychain enumeration while keeping the actual secret values in the system keychain.

### 3.4 Tauri Commands

```rust
#[tauri::command]
async fn get_secret(key: String) -> Result<String, String> {
    keychain::get_secret(&key)
}

#[tauri::command]
async fn set_secret(key: String, value: String) -> Result<(), String> {
    keychain::set_secret(&key, &value)?;
    keychain::add_to_index(&key)?;
    Ok(())
}

#[tauri::command]
async fn delete_secret(key: String) -> Result<(), String> {
    keychain::delete_secret(&key)?;
    keychain::remove_from_index(&key)?;
    Ok(())
}

#[tauri::command]
async fn list_secret_keys() -> Result<Vec<String>, String> {
    keychain::load_index()
}
```

### 3.5 Injecting Secrets into Agent PTY Sessions

When `PtyManager::spawn` is called, the backend:

1. Reads the secret key index.
2. For each key in the index, calls `keychain::get_secret(key)`.
3. Injects the key-value pairs as environment variables via `CommandBuilder::env(key, value)`.

This happens **automatically** on every spawn/restart — no frontend coordination needed.

**Agent-to-key mapping** (convention-based, no config needed):

| Agent | Required Key | Optional Keys |
|---|---|---|
| `claude` | `ANTHROPIC_API_KEY` | — |
| `codex` | `OPENAI_API_KEY` | — |
| `gemini` | `GOOGLE_API_KEY` | — |

All keys from the index are injected into all agents. The agent CLI ignores keys it does not recognize. This avoids maintaining a separate mapping table.

### 3.6 Memory Caching

To avoid repeated Keychain lookups on every spawn:

- On app startup, load all secrets from Keychain into an in-memory `HashMap<String, String>` behind a `Mutex`.
- `set_secret` and `delete_secret` update both the Keychain and the cache.
- `PtyManager::spawn` reads from the cache (fast, no Keychain I/O).
- Cache is never serialized to disk — it exists only in process memory.

### 3.7 Security Considerations

- Secret values are **never** sent to the frontend except through `get_secret`, which should only be called from the Settings modal for display (masked by default).
- The frontend receives `****` masked values by default. Full values are only revealed on explicit user action ("Show" button) which calls `get_secret` and displays briefly.
- API keys in the PTY environment are inherited by the child process only — they do not appear in IPC events or the terminal output stream.
- The secret key index file (`secret-keys.json`) contains only key names, never values.

---

## 4. Tauri Commands Summary

| Command | Parameters | Returns | Description |
|---|---|---|---|
| `list_shortcuts` | — | `ShortcutConfig` | Read shortcuts from JSON file |
| `save_shortcuts` | `shortcuts: ShortcutConfig` | `()` | Write shortcuts to JSON file |
| `get_secret` | `key: String` | `String` | Read secret value from Keychain |
| `set_secret` | `key: String, value: String` | `()` | Write secret to Keychain + update index |
| `delete_secret` | `key: String` | `()` | Remove secret from Keychain + update index |
| `list_secret_keys` | — | `Vec<String>` | List stored key names (no values) |
| `switch_agent_model` | `workspace_id: String, model: String` | `()` | Kill PTY, rebuild command with model flag, respawn |

### Rust type definitions

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConfig {
    pub version: u32,
    pub shortcuts: HashMap<String, String>,
}
```

---

## 5. Frontend Components

### 5.1 ShortcutHandler

**File:** `src/components/ShortcutHandler.tsx`

```typescript
// Invisible component — mounts a global keydown listener
export default function ShortcutHandler() {
  const [shortcutMap, setShortcutMap] = createSignal<Map<string, string>>(new Map());

  onMount(async () => {
    // Load initial shortcuts from backend
    const config = await invoke<ShortcutConfig>("list_shortcuts");
    setShortcutMap(buildMap(config.shortcuts));

    // Listen for hot-reload updates
    listen("shortcuts-updated", (event) => {
      setShortcutMap(buildMap(event.payload.shortcuts));
    });

    // Global keydown listener
    window.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    window.removeEventListener("keydown", handleKeyDown);
  });

  function handleKeyDown(e: KeyboardEvent) {
    const binding = normalizeEvent(e);
    const action = shortcutMap().get(binding);
    if (action && dispatchAction(action)) {
      e.preventDefault();
    }
  }

  return null;
}
```

### 5.2 ModelSelector

**File:** `src/components/ModelSelector.tsx`

Placed inside the `AgentPanel` header between the agent badge and status badge.

```
<span class="agent-badge">claude</span>
<ModelSelector
  models={agentModels()}
  current={ws().model}
  onSelect={(model) => switchModel(ws().id, model)}
  disabled={switching()}
/>
<span class="agent-status-badge">running</span>
```

The component renders:
- A button showing the current model name with a chevron.
- On click, a dropdown overlay anchored below the button.
- Each item in the dropdown shows the model name; the current model has a checkmark.
- Clicking a different model calls `onSelect`, which triggers the switch flow.

### 5.3 SettingsModal

**File:** `src/components/SettingsModal.tsx`

A full-screen modal with a tabbed interface:

```
┌──────────────────────────────────────────────┐
│  Settings                              [x]   │
│                                              │
│  [Secrets]  [Shortcuts]  [General]           │
│  ─────────────────────────────────────────── │
│                                              │
│  (tab content)                               │
│                                              │
└──────────────────────────────────────────────┘
```

**Secrets tab:** `SecretManager` component (see 5.4).
**Shortcuts tab:** Table of action → keybinding with edit capability. Conflicts highlighted in red.
**General tab:** Theme selector, max concurrent agents — placeholder for future settings.

Opened via `Cmd+,` shortcut (future) or a gear icon in the sidebar footer.

### 5.4 SecretManager

**File:** `src/components/SecretManager.tsx`

Within the Secrets tab of SettingsModal:

```
┌──────────────────────────────────────────────┐
│  API Keys                                    │
│                                              │
│  ANTHROPIC_API_KEY    ••••••••  [Show] [Del] │
│  OPENAI_API_KEY       ••••••••  [Show] [Del] │
│  GOOGLE_API_KEY       (not set)       [Add]  │
│                                              │
│  ── Add Custom Key ──                        │
│  Key:   [________________]                   │
│  Value: [________________]    [Save]         │
└──────────────────────────────────────────────┘
```

**Behavior:**
- On mount, calls `list_secret_keys()` to populate the list.
- Values are masked (`••••••••`) by default.
- "Show" button calls `get_secret(key)` and reveals the value for 5 seconds, then re-masks.
- "Del" button calls `delete_secret(key)` with confirmation dialog.
- "Add" form validates key name (uppercase, alphanumeric + underscore) and calls `set_secret(key, value)`.
- Suggested keys (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GOOGLE_API_KEY`) are always shown even if not set, with an "Add" button.

---

## 6. Performance Targets

| Metric | Target | How |
|---|---|---|
| Shortcut lookup | O(1) per keypress | `Map.get()` on normalized keybinding string |
| Shortcut handler overhead | < 1ms per keypress | Single listener, no DOM traversal, no event propagation unless matched |
| Model switch | < 2s end-to-end | Kill PTY (~50ms) + spawn new process (~100ms) + agent init (~1.5s) |
| Keychain read (cached) | < 1ms | In-memory `HashMap` lookup, no Keychain I/O after startup |
| Keychain read (cold) | < 50ms | Single `SecItemCopyMatching` call per key |
| Settings modal open | < 100ms | Lazy-loaded component, no heavy I/O on mount except `list_secret_keys` |

---

## 7. Files Changed

| File | Change |
|---|---|
| `src-tauri/Cargo.toml` | **Modified** — add `security-framework` dependency |
| `src-tauri/src/config.rs` | **Modified** — add `models`, `default_model`, `model_flag` to `AgentConfig`; add `ShortcutConfig` type |
| `src-tauri/src/agent.rs` | **Modified** — `build_command` takes optional `model` parameter |
| `src-tauri/src/keychain.rs` | **New** — Keychain access functions + index management |
| `src-tauri/src/pty_manager.rs` | **Modified** — inject secrets from cache on spawn; add `switch_model` method |
| `src-tauri/src/commands.rs` | **Modified** — add Tauri command handlers for shortcuts, secrets, model switching |
| `src-tauri/src/lib.rs` | **Modified** — register new commands, add keychain module |
| `src/lib/shortcut-parser.ts` | **New** — parse shortcut config into lookup Map |
| `src/lib/action-registry.ts` | **New** — action name to handler mapping |
| `src/components/ShortcutHandler.tsx` | **New** — global keydown listener component |
| `src/components/ModelSelector.tsx` | **New** — model dropdown in agent header |
| `src/components/SettingsModal.tsx` | **New** — tabbed settings modal |
| `src/components/SecretManager.tsx` | **New** — API key CRUD within settings |
| `src/components/TabBar.tsx` | **Modified** — remove hardcoded keyboard handler; register actions via action-registry |
| `src/components/AgentPanel.tsx` | **Modified** — add ModelSelector to header; register `message.send` action |
| `src/store/workspace-store.ts` | **Modified** — add `model` field to `WorkspaceTab`; add `setWorkspaceModel` action |
| `src/App.tsx` | **Modified** — mount `ShortcutHandler`; mount `SettingsModal` |

---

## 8. Migration Path

### Step 1: Shortcut system (backend)
- Add `ShortcutConfig` type to `config.rs`.
- Add `list_shortcuts` / `save_shortcuts` Tauri commands.
- Add file watcher for `shortcuts.json` + `shortcuts-updated` event emission.

### Step 2: Shortcut system (frontend)
- Create `shortcut-parser.ts` and `action-registry.ts`.
- Create `ShortcutHandler.tsx`, mount in `App.tsx`.
- Refactor `TabBar.tsx` to register actions via registry instead of direct listener.
- Register `message.send` in `AgentPanel.tsx`.

### Step 3: Model selector (backend)
- Extend `AgentConfig` with `models`, `default_model`, `model_flag`.
- Update `AgentAdapter::build_command` to accept model parameter.
- Add `switch_agent_model` Tauri command (kill + respawn).
- Update default config with model lists.

### Step 4: Model selector (frontend)
- Add `model` field to `WorkspaceTab` and `setWorkspaceModel` action.
- Create `ModelSelector.tsx`.
- Integrate into `AgentPanel` header.

### Step 5: Keychain (backend)
- Add `security-framework` dependency.
- Create `keychain.rs` with get/set/delete + index management.
- Add in-memory secret cache with `Mutex<HashMap>`.
- Wire `PtyManager::spawn` to inject cached secrets.
- Add Tauri commands for secret CRUD.

### Step 6: Settings modal (frontend)
- Create `SettingsModal.tsx` with tabs.
- Create `SecretManager.tsx` for the Secrets tab.
- Add Shortcuts tab showing current bindings + conflicts.
- Wire open/close to `settings.open` action.
