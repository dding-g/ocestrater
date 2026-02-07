import { createSignal, createMemo, onMount, For, Show, onCleanup } from "solid-js";
import {
  listShortcuts,
  listSecretKeys,
  getSecret,
  setSecret,
  deleteSecret,
} from "../lib/tauri";

type Tab = "secrets" | "shortcuts" | "general";

const SHORTCUT_DESCRIPTIONS: Record<string, string> = {
  "workspace.new": "Open new workspace dialog",
  "workspace.close": "Close active tab",
  "tab.1": "Switch to tab 1",
  "tab.2": "Switch to tab 2",
  "tab.3": "Switch to tab 3",
  "tab.4": "Switch to tab 4",
  "tab.5": "Switch to tab 5",
  "tab.6": "Switch to tab 6",
  "tab.7": "Switch to tab 7",
  "tab.8": "Switch to tab 8",
  "tab.9": "Switch to tab 9",
  "tab.next": "Cycle to next tab",
  "tab.prev": "Cycle to previous tab",
  "palette.snippets": "Open snippet palette",
  "message.send": "Send message in input bar",
  "palette.command": "Open command palette",
  "settings.open": "Open settings",
  "agent.restart": "Restart active agent",
};

const SUGGESTED_KEYS = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GOOGLE_API_KEY"];

interface Props {
  open: boolean;
  onClose: () => void;
}

export default function SettingsModal(props: Props) {
  const [tab, setTab] = createSignal<Tab>("secrets");
  const [shortcuts, setShortcuts] = createSignal<Record<string, string>>({});
  const [secretKeys, setSecretKeys] = createSignal<string[]>([]);
  const [revealedSecrets, setRevealedSecrets] = createSignal<Record<string, string>>({});
  const [newKeyName, setNewKeyName] = createSignal("");
  const [newKeyValue, setNewKeyValue] = createSignal("");
  const [confirmDelete, setConfirmDelete] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [editingKey, setEditingKey] = createSignal<string | null>(null);
  const [editingValue, setEditingValue] = createSignal("");

  onMount(() => {
    loadData();
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      props.onClose();
    }
  }

  async function loadData() {
    setLoading(true);
    try {
      const [shortcutConfig, keys] = await Promise.all([
        listShortcuts().catch(() => ({ version: 1, shortcuts: {} })),
        listSecretKeys().catch(() => []),
      ]);
      setShortcuts(shortcutConfig.shortcuts);
      setSecretKeys(keys);
    } finally {
      setLoading(false);
    }
  }

  const allSecretKeys = createMemo((): string[] => {
    const all = [...SUGGESTED_KEYS];
    for (const key of secretKeys()) {
      if (!all.includes(key)) {
        all.push(key);
      }
    }
    return all;
  });

  function isKeyStored(key: string): boolean {
    return secretKeys().includes(key);
  }

  async function handleReveal(key: string) {
    try {
      const value = await getSecret(key);
      setRevealedSecrets((prev) => ({ ...prev, [key]: value }));
      // Auto-hide after 5 seconds
      setTimeout(() => {
        setRevealedSecrets((prev) => {
          const next = { ...prev };
          delete next[key];
          return next;
        });
      }, 5000);
    } catch {
      // Failed to read secret
    }
  }

  function handleHide(key: string) {
    setRevealedSecrets((prev) => {
      const next = { ...prev };
      delete next[key];
      return next;
    });
  }

  async function handleDelete(key: string) {
    if (confirmDelete() !== key) {
      setConfirmDelete(key);
      return;
    }
    try {
      await deleteSecret(key);
      setSecretKeys((prev) => prev.filter((k) => k !== key));
      setRevealedSecrets((prev) => {
        const next = { ...prev };
        delete next[key];
        return next;
      });
    } catch {
      // Failed
    }
    setConfirmDelete(null);
  }

  async function handleAddSecret() {
    const name = newKeyName().trim().toUpperCase();
    const value = newKeyValue().trim();
    if (!name || !value) return;
    if (!/^[A-Z][A-Z0-9_]*$/.test(name)) {
      alert("Key name must be uppercase letters, digits, and underscores");
      return;
    }
    try {
      await setSecret(name, value);
      if (!secretKeys().includes(name)) {
        setSecretKeys((prev) => [...prev, name]);
      }
      setNewKeyName("");
      setNewKeyValue("");
    } catch {
      // Failed
    }
  }

  function startEditSuggested(key: string) {
    setEditingKey(key);
    setEditingValue("");
  }

  function cancelEditSuggested() {
    setEditingKey(null);
    setEditingValue("");
  }

  async function saveEditSuggested() {
    const key = editingKey();
    const value = editingValue().trim();
    if (!key || !value) return;
    try {
      await setSecret(key, value);
      if (!secretKeys().includes(key)) {
        setSecretKeys((prev) => [...prev, key]);
      }
    } catch {
      // Failed
    }
    setEditingKey(null);
    setEditingValue("");
  }

  return (
    <Show when={props.open}>
      <div
        class="settings-overlay"
        onClick={props.onClose}
        onKeyDown={handleKeyDown}
      >
        <div class="settings-modal" onClick={(e) => e.stopPropagation()}>
          <div class="settings-header">
            <span class="settings-title">Settings</span>
            <button class="settings-close" onClick={props.onClose}>
              &times;
            </button>
          </div>

          <div class="settings-tabs">
            <button
              class="settings-tab"
              classList={{ active: tab() === "secrets" }}
              onClick={() => setTab("secrets")}
            >
              Secrets
            </button>
            <button
              class="settings-tab"
              classList={{ active: tab() === "shortcuts" }}
              onClick={() => setTab("shortcuts")}
            >
              Shortcuts
            </button>
            <button
              class="settings-tab"
              classList={{ active: tab() === "general" }}
              onClick={() => setTab("general")}
            >
              General
            </button>
          </div>

          <div class="settings-content">
            <Show when={loading()}>
              <div class="settings-loading">Loading...</div>
            </Show>
            {/* Secrets Tab */}
            <Show when={tab() === "secrets"}>
              <div class="secrets-section">
                <h3 class="section-heading">API Keys</h3>
                <div class="secret-list">
                  <For each={allSecretKeys()}>
                    {(key) => (
                      <div class="secret-row">
                        <span class="secret-name">{key}</span>
                        <Show
                          when={isKeyStored(key)}
                          fallback={
                            <div class="secret-actions">
                              <Show
                                when={editingKey() === key}
                                fallback={
                                  <>
                                    <span class="secret-not-set">(not set)</span>
                                    <button
                                      class="secret-btn"
                                      onClick={() => startEditSuggested(key)}
                                    >
                                      Add
                                    </button>
                                  </>
                                }
                              >
                                <input
                                  class="secret-input secret-inline-input"
                                  type="password"
                                  placeholder={`Enter value for ${key}`}
                                  value={editingValue()}
                                  onInput={(e) => setEditingValue(e.currentTarget.value)}
                                  onKeyDown={(e) => {
                                    if (e.key === "Enter") saveEditSuggested();
                                    else if (e.key === "Escape") cancelEditSuggested();
                                  }}
                                  ref={(el) => setTimeout(() => el.focus(), 0)}
                                />
                                <button
                                  class="secret-btn primary"
                                  onClick={saveEditSuggested}
                                  disabled={!editingValue().trim()}
                                >
                                  Save
                                </button>
                                <button
                                  class="secret-btn"
                                  onClick={cancelEditSuggested}
                                >
                                  Cancel
                                </button>
                              </Show>
                            </div>
                          }
                        >
                          <div class="secret-actions">
                            <span class="secret-value">
                              {revealedSecrets()[key] || "\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022"}
                            </span>
                            <Show
                              when={revealedSecrets()[key]}
                              fallback={
                                <button
                                  class="secret-btn"
                                  onClick={() => handleReveal(key)}
                                >
                                  Show
                                </button>
                              }
                            >
                              <button
                                class="secret-btn"
                                onClick={() => handleHide(key)}
                              >
                                Hide
                              </button>
                            </Show>
                            <button
                              class="secret-btn danger"
                              onClick={() => handleDelete(key)}
                            >
                              {confirmDelete() === key ? "Confirm" : "Del"}
                            </button>
                          </div>
                        </Show>
                      </div>
                    )}
                  </For>
                </div>

                <div class="secret-add-section">
                  <h4 class="subsection-heading">Add Custom Key</h4>
                  <div class="secret-add-form">
                    <input
                      class="secret-input"
                      placeholder="KEY_NAME"
                      value={newKeyName()}
                      onInput={(e) => setNewKeyName(e.currentTarget.value)}
                    />
                    <input
                      class="secret-input"
                      type="password"
                      placeholder="Value"
                      value={newKeyValue()}
                      onInput={(e) => setNewKeyValue(e.currentTarget.value)}
                    />
                    <button
                      class="secret-btn primary"
                      onClick={handleAddSecret}
                      disabled={!newKeyName().trim() || !newKeyValue().trim()}
                    >
                      Save
                    </button>
                  </div>
                </div>
              </div>
            </Show>

            {/* Shortcuts Tab */}
            <Show when={tab() === "shortcuts"}>
              <div class="shortcuts-section">
                <h3 class="section-heading">Keyboard Shortcuts</h3>
                <p class="section-note">
                  Edit ~/.ocestrater/shortcuts.json to customize bindings.
                </p>
                <div class="shortcut-list">
                  <div class="shortcut-row header">
                    <span class="shortcut-action">Action</span>
                    <span class="shortcut-binding">Binding</span>
                    <span class="shortcut-desc">Description</span>
                  </div>
                  <For each={Object.entries(shortcuts())}>
                    {([action, binding]) => (
                      <div class="shortcut-row">
                        <span class="shortcut-action">{action}</span>
                        <span class="shortcut-binding">
                          <kbd>{binding}</kbd>
                        </span>
                        <span class="shortcut-desc">
                          {SHORTCUT_DESCRIPTIONS[action] || ""}
                        </span>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>

            {/* General Tab */}
            <Show when={tab() === "general"}>
              <div class="general-section">
                <h3 class="section-heading">General</h3>
                <p class="section-note">
                  Additional settings will be available in future releases.
                </p>
              </div>
            </Show>
          </div>
        </div>
      </div>

      <style>{`
        .settings-overlay {
          position: fixed;
          inset: 0;
          background: rgba(0, 0, 0, 0.5);
          display: flex;
          align-items: center;
          justify-content: center;
          z-index: 2000;
        }
        .settings-modal {
          width: 600px;
          max-width: 90vw;
          max-height: 80vh;
          background: var(--bg-secondary);
          border: 1px solid var(--border);
          border-radius: 8px;
          display: flex;
          flex-direction: column;
          box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
        }
        .settings-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 14px 18px;
          border-bottom: 1px solid var(--border);
        }
        .settings-title {
          font-weight: 600;
          font-size: 14px;
        }
        .settings-close {
          font-size: 20px;
          line-height: 1;
          color: var(--text-muted);
          padding: 2px 6px;
          border-radius: var(--radius);
        }
        .settings-close:hover {
          color: var(--text-primary);
          background: var(--bg-hover);
        }
        .settings-tabs {
          display: flex;
          gap: 0;
          border-bottom: 1px solid var(--border);
          padding: 0 18px;
        }
        .settings-tab {
          padding: 10px 16px;
          font-size: 12px;
          font-weight: 500;
          color: var(--text-secondary);
          border-bottom: 2px solid transparent;
          margin-bottom: -1px;
        }
        .settings-tab:hover {
          color: var(--text-primary);
        }
        .settings-tab.active {
          color: var(--text-primary);
          border-bottom-color: var(--accent);
        }
        .settings-content {
          flex: 1;
          overflow-y: auto;
          padding: 18px;
        }
        .section-heading {
          font-size: 13px;
          font-weight: 600;
          margin-bottom: 12px;
        }
        .subsection-heading {
          font-size: 12px;
          font-weight: 500;
          color: var(--text-secondary);
          margin-bottom: 8px;
        }
        .section-note {
          font-size: 12px;
          color: var(--text-muted);
          margin-bottom: 14px;
        }

        .settings-loading {
          text-align: center;
          padding: 12px;
          font-size: 12px;
          color: var(--text-muted);
        }

        /* Secrets */
        .secret-list {
          display: flex;
          flex-direction: column;
          gap: 4px;
          margin-bottom: 18px;
        }
        .secret-row {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-radius: var(--radius);
        }
        .secret-name {
          font-family: var(--font-mono);
          font-size: 12px;
          font-weight: 500;
          flex-shrink: 0;
        }
        .secret-actions {
          display: flex;
          align-items: center;
          gap: 8px;
        }
        .secret-value {
          font-family: var(--font-mono);
          font-size: 11px;
          color: var(--text-muted);
          max-width: 180px;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
        .secret-not-set {
          font-size: 11px;
          color: var(--text-muted);
          font-style: italic;
        }
        .secret-btn {
          font-size: 11px;
          padding: 3px 10px;
          border-radius: 4px;
          background: var(--bg-hover);
          color: var(--text-secondary);
        }
        .secret-btn:hover {
          background: var(--border);
          color: var(--text-primary);
        }
        .secret-btn.primary {
          background: var(--accent);
          color: #fff;
        }
        .secret-btn.primary:hover:not(:disabled) {
          background: var(--accent-hover);
        }
        .secret-btn.primary:disabled {
          opacity: 0.4;
          cursor: default;
        }
        .secret-btn.danger {
          color: var(--error);
        }
        .secret-btn.danger:hover {
          background: rgba(244, 67, 54, 0.15);
        }
        .secret-inline-input {
          width: 160px;
          font-size: 11px;
          padding: 3px 8px;
        }
        .secret-add-section {
          border-top: 1px solid var(--border);
          padding-top: 14px;
        }
        .secret-add-form {
          display: flex;
          gap: 8px;
          align-items: center;
        }
        .secret-input {
          flex: 1;
          font-size: 12px;
          padding: 6px 10px;
        }

        /* Shortcuts */
        .shortcut-list {
          display: flex;
          flex-direction: column;
          gap: 2px;
        }
        .shortcut-row {
          display: flex;
          align-items: center;
          padding: 6px 12px;
          border-radius: var(--radius);
        }
        .shortcut-row:not(.header):hover {
          background: var(--bg-tertiary);
        }
        .shortcut-row.header {
          font-size: 11px;
          font-weight: 600;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
          padding-bottom: 8px;
          border-bottom: 1px solid var(--border);
          margin-bottom: 4px;
        }
        .shortcut-action {
          width: 160px;
          font-family: var(--font-mono);
          font-size: 11px;
          flex-shrink: 0;
        }
        .shortcut-binding {
          width: 160px;
          flex-shrink: 0;
        }
        .shortcut-binding kbd {
          font-family: var(--font-mono);
          font-size: 11px;
          padding: 2px 6px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: 4px;
        }
        .shortcut-desc {
          font-size: 12px;
          color: var(--text-secondary);
          flex: 1;
        }

        /* General */
        .general-section {
          color: var(--text-secondary);
        }
      `}</style>
    </Show>
  );
}
