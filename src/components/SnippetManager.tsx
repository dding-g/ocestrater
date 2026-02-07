import { createSignal, createEffect, For, Show } from "solid-js";
import type { Snippet, SnippetCategory } from "../lib/types";
import { listSnippets, saveSnippet, deleteSnippet } from "../lib/tauri";

interface SnippetManagerProps {
  repoPath: string | null;
  onClose: () => void;
}

const CATEGORY_OPTIONS: SnippetCategory[] = ["setup", "build", "test", "lint", "deploy", "custom"];

const EMPTY_SNIPPET: Snippet = {
  name: "",
  command: "",
  description: "",
  category: "custom",
  keybinding: null,
};

export default function SnippetManager(props: SnippetManagerProps) {
  const [scope, setScope] = createSignal<"global" | "repo">(props.repoPath ? "repo" : "global");
  const [snippets, setSnippets] = createSignal<Snippet[]>([]);
  const [editingName, setEditingName] = createSignal<string | null>(null);
  const [editDraft, setEditDraft] = createSignal<Snippet>(EMPTY_SNIPPET);
  const [showAdd, setShowAdd] = createSignal(false);
  const [newSnippet, setNewSnippet] = createSignal<Snippet>({ ...EMPTY_SNIPPET });
  const [deleteConfirm, setDeleteConfirm] = createSignal<string | null>(null);

  const effectiveRepoPath = () => (scope() === "repo" ? props.repoPath : null);

  function loadSnippets() {
    const rp = effectiveRepoPath() ?? undefined;
    listSnippets(rp)
      .then(setSnippets)
      .catch(() => setSnippets([]));
  }

  createEffect(() => {
    scope();
    loadSnippets();
  });

  function startEdit(snippet: Snippet) {
    setEditingName(snippet.name);
    setEditDraft({ ...snippet });
  }

  function cancelEdit() {
    setEditingName(null);
  }

  async function saveEdit() {
    const draft = editDraft();
    if (!draft.name.trim() || !draft.command.trim()) return;
    try {
      await saveSnippet(effectiveRepoPath() ?? null, draft);
      setEditingName(null);
      loadSnippets();
    } catch {
      // Save failed
    }
  }

  async function handleDelete(name: string) {
    try {
      await deleteSnippet(effectiveRepoPath() ?? null, name);
      setDeleteConfirm(null);
      loadSnippets();
    } catch {
      // Delete failed
    }
  }

  async function handleAddSave() {
    const s = newSnippet();
    if (!s.name.trim() || !s.command.trim()) return;
    try {
      await saveSnippet(effectiveRepoPath() ?? null, s);
      setNewSnippet({ ...EMPTY_SNIPPET });
      setShowAdd(false);
      loadSnippets();
    } catch {
      // Save failed
    }
  }

  return (
    <div class="sm-overlay" onClick={props.onClose}>
      <div class="sm-modal" onClick={(e) => e.stopPropagation()}>
        <div class="sm-header">
          <span class="sm-title">Snippet Manager</span>
          <button class="sm-close" onClick={props.onClose}>&times;</button>
        </div>

        <div class="sm-toolbar">
          <div class="sm-scope-select">
            <button
              class="sm-scope-btn"
              classList={{ active: scope() === "global" }}
              onClick={() => setScope("global")}
            >
              Global
            </button>
            <Show when={props.repoPath}>
              <button
                class="sm-scope-btn"
                classList={{ active: scope() === "repo" }}
                onClick={() => setScope("repo")}
              >
                Repo
              </button>
            </Show>
          </div>
          <button class="sm-add-btn" onClick={() => setShowAdd(true)}>
            + Add Snippet
          </button>
        </div>

        <div class="sm-content">
          <Show
            when={snippets().length > 0}
            fallback={
              <div class="sm-empty">
                No snippets in {scope()} scope.
              </div>
            }
          >
            <table class="sm-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Command</th>
                  <th>Category</th>
                  <th>Keybinding</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                <For each={snippets()}>
                  {(snippet) => (
                    <Show
                      when={editingName() === snippet.name}
                      fallback={
                        <tr class="sm-row">
                          <td class="sm-cell-name">{snippet.name}</td>
                          <td class="sm-cell-cmd">
                            <code>{snippet.command}</code>
                          </td>
                          <td>
                            <span class={`sp-cat-badge sp-cat-${snippet.category}`}>
                              {snippet.category}
                            </span>
                          </td>
                          <td class="sm-cell-key">
                            {snippet.keybinding ?? "--"}
                          </td>
                          <td class="sm-cell-actions">
                            <button class="sm-action-btn" onClick={() => startEdit(snippet)}>
                              Edit
                            </button>
                            <Show
                              when={deleteConfirm() === snippet.name}
                              fallback={
                                <button
                                  class="sm-action-btn sm-action-delete"
                                  onClick={() => setDeleteConfirm(snippet.name)}
                                >
                                  Delete
                                </button>
                              }
                            >
                              <button
                                class="sm-action-btn sm-action-delete"
                                onClick={() => handleDelete(snippet.name)}
                              >
                                Confirm
                              </button>
                              <button
                                class="sm-action-btn"
                                onClick={() => setDeleteConfirm(null)}
                              >
                                Cancel
                              </button>
                            </Show>
                          </td>
                        </tr>
                      }
                    >
                      <tr class="sm-row sm-row-editing">
                        <td>
                          <input
                            class="sm-edit-input"
                            value={editDraft().name}
                            onInput={(e) =>
                              setEditDraft((d) => ({ ...d, name: e.currentTarget.value }))
                            }
                          />
                        </td>
                        <td>
                          <textarea
                            class="sm-edit-textarea"
                            value={editDraft().command}
                            onInput={(e) =>
                              setEditDraft((d) => ({ ...d, command: e.currentTarget.value }))
                            }
                            rows={2}
                          />
                        </td>
                        <td>
                          <select
                            class="sm-edit-select"
                            value={editDraft().category}
                            onChange={(e) =>
                              setEditDraft((d) => ({
                                ...d,
                                category: e.currentTarget.value as SnippetCategory,
                              }))
                            }
                          >
                            <For each={CATEGORY_OPTIONS}>
                              {(cat) => <option value={cat}>{cat}</option>}
                            </For>
                          </select>
                        </td>
                        <td>
                          <input
                            class="sm-edit-input"
                            value={editDraft().keybinding ?? ""}
                            placeholder="e.g. Ctrl+Shift+T"
                            onInput={(e) =>
                              setEditDraft((d) => ({
                                ...d,
                                keybinding: e.currentTarget.value || null,
                              }))
                            }
                          />
                        </td>
                        <td class="sm-cell-actions">
                          <button class="sm-action-btn sm-action-save" onClick={saveEdit}>
                            Save
                          </button>
                          <button class="sm-action-btn" onClick={cancelEdit}>
                            Cancel
                          </button>
                        </td>
                      </tr>
                    </Show>
                  )}
                </For>
              </tbody>
            </table>
          </Show>

          <Show when={showAdd()}>
            <div class="sm-add-form">
              <div class="sm-add-form-title">New Snippet</div>
              <div class="sm-add-fields">
                <div class="sm-field">
                  <label>Name *</label>
                  <input
                    value={newSnippet().name}
                    onInput={(e) =>
                      setNewSnippet((s) => ({ ...s, name: e.currentTarget.value }))
                    }
                    placeholder="e.g. test"
                  />
                </div>
                <div class="sm-field">
                  <label>Command *</label>
                  <textarea
                    value={newSnippet().command}
                    onInput={(e) =>
                      setNewSnippet((s) => ({ ...s, command: e.currentTarget.value }))
                    }
                    placeholder="e.g. cargo test"
                    rows={3}
                  />
                </div>
                <div class="sm-field">
                  <label>Description</label>
                  <input
                    value={newSnippet().description}
                    onInput={(e) =>
                      setNewSnippet((s) => ({ ...s, description: e.currentTarget.value }))
                    }
                    placeholder="Run all unit tests"
                  />
                </div>
                <div class="sm-field-row">
                  <div class="sm-field">
                    <label>Category</label>
                    <select
                      class="sm-edit-select"
                      value={newSnippet().category}
                      onChange={(e) =>
                        setNewSnippet((s) => ({
                          ...s,
                          category: e.currentTarget.value as SnippetCategory,
                        }))
                      }
                    >
                      <For each={CATEGORY_OPTIONS}>
                        {(cat) => <option value={cat}>{cat}</option>}
                      </For>
                    </select>
                  </div>
                  <div class="sm-field">
                    <label>Keybinding</label>
                    <input
                      value={newSnippet().keybinding ?? ""}
                      onInput={(e) =>
                        setNewSnippet((s) => ({
                          ...s,
                          keybinding: e.currentTarget.value || null,
                        }))
                      }
                      placeholder="Ctrl+Shift+T"
                    />
                  </div>
                </div>
              </div>
              <div class="sm-add-actions">
                <button class="sm-btn sm-btn-primary" onClick={handleAddSave}>
                  Save Snippet
                </button>
                <button
                  class="sm-btn sm-btn-secondary"
                  onClick={() => {
                    setShowAdd(false);
                    setNewSnippet({ ...EMPTY_SNIPPET });
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          </Show>
        </div>
      </div>

      <style>{`
        .sm-overlay {
          position: fixed;
          inset: 0;
          z-index: 2000;
          background: rgba(0, 0, 0, 0.5);
          backdrop-filter: blur(4px);
          display: flex;
          align-items: center;
          justify-content: center;
        }
        .sm-modal {
          width: 720px;
          max-height: 80vh;
          background: var(--bg-secondary);
          border: 1px solid var(--border);
          border-radius: 12px;
          box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
          display: flex;
          flex-direction: column;
          overflow: hidden;
        }
        .sm-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 12px 16px;
          border-bottom: 1px solid var(--border);
        }
        .sm-title {
          font-weight: 600;
          font-size: 14px;
        }
        .sm-close {
          font-size: 18px;
          color: var(--text-muted);
          width: 24px;
          height: 24px;
          display: flex;
          align-items: center;
          justify-content: center;
          border-radius: var(--radius);
        }
        .sm-close:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
        .sm-toolbar {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 8px 16px;
          border-bottom: 1px solid var(--border);
        }
        .sm-scope-select {
          display: flex;
          gap: 4px;
        }
        .sm-scope-btn {
          padding: 4px 12px;
          font-size: 12px;
          border-radius: var(--radius);
          background: var(--bg-tertiary);
          color: var(--text-secondary);
        }
        .sm-scope-btn:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
        .sm-scope-btn.active {
          background: var(--accent);
          color: #fff;
        }
        .sm-add-btn {
          padding: 4px 12px;
          font-size: 12px;
          border-radius: var(--radius);
          background: var(--accent);
          color: #fff;
          font-weight: 500;
        }
        .sm-add-btn:hover {
          background: var(--accent-hover);
        }
        .sm-content {
          flex: 1;
          overflow-y: auto;
          padding: 8px;
        }
        .sm-empty {
          padding: 40px 16px;
          text-align: center;
          color: var(--text-muted);
          font-size: 13px;
        }
        .sm-table {
          width: 100%;
          border-collapse: collapse;
          font-size: 12px;
        }
        .sm-table th {
          text-align: left;
          padding: 6px 8px;
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-muted);
          border-bottom: 1px solid var(--border);
          font-weight: 500;
        }
        .sm-table td {
          padding: 6px 8px;
          vertical-align: middle;
          border-bottom: 1px solid var(--border);
        }
        .sm-row:hover {
          background: var(--bg-hover);
        }
        .sm-cell-name {
          font-weight: 600;
          font-family: var(--font-mono);
          font-size: 12px;
        }
        .sm-cell-cmd code {
          font-family: var(--font-mono);
          font-size: 11px;
          color: var(--text-secondary);
          max-width: 200px;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
          display: inline-block;
        }
        .sm-cell-key {
          font-family: var(--font-mono);
          font-size: 11px;
          color: var(--text-muted);
        }
        .sm-cell-actions {
          display: flex;
          gap: 4px;
          white-space: nowrap;
        }
        .sm-action-btn {
          padding: 2px 8px;
          font-size: 11px;
          border-radius: 4px;
          background: var(--bg-tertiary);
          color: var(--text-secondary);
        }
        .sm-action-btn:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
        .sm-action-delete {
          color: var(--error);
        }
        .sm-action-delete:hover {
          background: rgba(244, 67, 54, 0.15);
          color: var(--error);
        }
        .sm-action-save {
          background: var(--accent);
          color: #fff;
        }
        .sm-action-save:hover {
          background: var(--accent-hover);
        }
        .sm-edit-input {
          width: 100%;
          padding: 4px 6px;
          font-size: 11px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: 4px;
          color: var(--text-primary);
          font-family: var(--font-mono);
        }
        .sm-edit-input:focus {
          border-color: var(--accent);
          outline: none;
        }
        .sm-edit-textarea {
          width: 100%;
          padding: 4px 6px;
          font-size: 11px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: 4px;
          color: var(--text-primary);
          font-family: var(--font-mono);
          resize: vertical;
        }
        .sm-edit-textarea:focus {
          border-color: var(--accent);
          outline: none;
        }
        .sm-edit-select {
          padding: 4px 6px;
          font-size: 11px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: 4px;
          color: var(--text-primary);
        }

        /* Category badge styles (.sp-cat-*) are in global.css */

        /* Add form */
        .sm-add-form {
          border-top: 1px solid var(--border);
          padding: 12px;
          margin-top: 8px;
        }
        .sm-add-form-title {
          font-weight: 600;
          font-size: 13px;
          margin-bottom: 10px;
        }
        .sm-add-fields {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }
        .sm-field {
          display: flex;
          flex-direction: column;
          gap: 4px;
          flex: 1;
        }
        .sm-field label {
          font-size: 11px;
          color: var(--text-secondary);
          font-weight: 500;
        }
        .sm-field input,
        .sm-field textarea {
          padding: 6px 8px;
          font-size: 12px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: var(--radius);
          color: var(--text-primary);
          font-family: var(--font-mono);
        }
        .sm-field input:focus,
        .sm-field textarea:focus {
          border-color: var(--accent);
          outline: none;
        }
        .sm-field textarea {
          resize: vertical;
        }
        .sm-field-row {
          display: flex;
          gap: 12px;
        }
        .sm-add-actions {
          display: flex;
          gap: 8px;
          margin-top: 12px;
        }
        .sm-btn {
          padding: 6px 16px;
          font-size: 12px;
          border-radius: var(--radius);
          font-weight: 500;
        }
        .sm-btn-primary {
          background: var(--accent);
          color: #fff;
        }
        .sm-btn-primary:hover {
          background: var(--accent-hover);
        }
        .sm-btn-secondary {
          background: var(--bg-tertiary);
          color: var(--text-secondary);
        }
        .sm-btn-secondary:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
      `}</style>
    </div>
  );
}
