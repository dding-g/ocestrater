import { createSignal, createEffect, For, Show, onCleanup } from "solid-js";
import type { Snippet, SnippetCategory } from "../lib/types";
import { listSnippets, runSnippetV2 } from "../lib/tauri";

interface SnippetPaletteProps {
  workspaceId: string;
  repoPath: string;
  onClose: () => void;
}

const CATEGORIES: { label: string; value: SnippetCategory | "all" }[] = [
  { label: "All", value: "all" },
  { label: "Setup", value: "setup" },
  { label: "Build", value: "build" },
  { label: "Test", value: "test" },
  { label: "Lint", value: "lint" },
  { label: "Deploy", value: "deploy" },
  { label: "Custom", value: "custom" },
];

export default function SnippetPalette(props: SnippetPaletteProps) {
  const [query, setQuery] = createSignal("");
  const [category, setCategory] = createSignal<SnippetCategory | "all">("all");
  const [snippets, setSnippets] = createSignal<Snippet[]>([]);
  const [selectedIndex, setSelectedIndex] = createSignal(0);
  let inputRef!: HTMLInputElement;

  // Load snippets when palette opens
  createEffect(() => {
    listSnippets(props.repoPath)
      .then(setSnippets)
      .catch(() => setSnippets([]));
  });

  // Focus input on mount
  createEffect(() => {
    if (inputRef) {
      inputRef.focus();
    }
  });

  const filtered = () => {
    const q = query().toLowerCase();
    const cat = category();
    return snippets().filter((s) => {
      if (cat !== "all" && s.category !== cat) return false;
      if (!q) return true;
      return (
        s.name.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        s.command.toLowerCase().includes(q)
      );
    });
  };

  // Reset selection when filter changes
  createEffect(() => {
    filtered();
    setSelectedIndex(0);
  });

  function handleKeyDown(e: KeyboardEvent) {
    const items = filtered();
    if (e.key === "Escape") {
      e.preventDefault();
      props.onClose();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => Math.min(i + 1, items.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const item = items[selectedIndex()];
      if (item) runSelected(item);
    }
  }

  async function runSelected(snippet: Snippet) {
    props.onClose();
    try {
      await runSnippetV2(props.workspaceId, snippet.name);
    } catch {
      // Error handling done via IPC events
    }
  }

  function categoryBadgeClass(cat: SnippetCategory): string {
    return `sp-cat-badge sp-cat-${cat}`;
  }

  return (
    <div class="sp-overlay" onClick={props.onClose}>
      <div class="sp-modal" onClick={(e) => e.stopPropagation()} onKeyDown={handleKeyDown}>
        <div class="sp-search-row">
          <input
            ref={inputRef}
            class="sp-search"
            type="text"
            placeholder="Search snippets..."
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
        </div>

        <div class="sp-categories">
          <For each={CATEGORIES}>
            {(cat) => (
              <button
                class="sp-cat-pill"
                classList={{ active: category() === cat.value }}
                onClick={() => setCategory(cat.value)}
              >
                {cat.label}
              </button>
            )}
          </For>
        </div>

        <div class="sp-list">
          <Show
            when={filtered().length > 0}
            fallback={
              <div class="sp-empty">
                No snippets found. Open Snippet Manager to add some.
              </div>
            }
          >
            <For each={filtered()}>
              {(snippet, index) => (
                <button
                  class="sp-item"
                  classList={{ selected: index() === selectedIndex() }}
                  onClick={() => runSelected(snippet)}
                  onMouseEnter={() => setSelectedIndex(index())}
                >
                  <div class="sp-item-main">
                    <span class="sp-item-name">{snippet.name}</span>
                    <Show when={snippet.description}>
                      <span class="sp-item-desc">{snippet.description}</span>
                    </Show>
                  </div>
                  <div class="sp-item-meta">
                    <span class={categoryBadgeClass(snippet.category)}>
                      {snippet.category}
                    </span>
                    <Show when={snippet.keybinding}>
                      <span class="sp-item-key">{snippet.keybinding}</span>
                    </Show>
                  </div>
                  <div class="sp-item-cmd">{snippet.command}</div>
                </button>
              )}
            </For>
          </Show>
        </div>
      </div>

      <style>{`
        .sp-overlay {
          position: fixed;
          inset: 0;
          z-index: 2000;
          background: rgba(0, 0, 0, 0.5);
          backdrop-filter: blur(4px);
          display: flex;
          align-items: flex-start;
          justify-content: center;
          padding-top: 15vh;
        }
        .sp-modal {
          width: 520px;
          max-height: 60vh;
          background: var(--bg-secondary);
          border: 1px solid var(--border);
          border-radius: 12px;
          box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
          display: flex;
          flex-direction: column;
          overflow: hidden;
        }
        .sp-search-row {
          padding: 12px 16px 8px;
        }
        .sp-search {
          width: 100%;
          padding: 10px 14px;
          font-size: 14px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: var(--radius);
          color: var(--text-primary);
          outline: none;
        }
        .sp-search:focus {
          border-color: var(--accent);
        }
        .sp-categories {
          display: flex;
          gap: 6px;
          padding: 4px 16px 8px;
          overflow-x: auto;
        }
        .sp-cat-pill {
          padding: 3px 10px;
          font-size: 11px;
          border-radius: 10px;
          background: var(--bg-tertiary);
          color: var(--text-secondary);
          white-space: nowrap;
          flex-shrink: 0;
        }
        .sp-cat-pill:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
        .sp-cat-pill.active {
          background: var(--accent);
          color: #fff;
        }
        .sp-list {
          flex: 1;
          overflow-y: auto;
          padding: 0 8px 8px;
        }
        .sp-empty {
          padding: 32px 16px;
          text-align: center;
          color: var(--text-muted);
          font-size: 13px;
        }
        .sp-item {
          display: flex;
          flex-direction: column;
          gap: 2px;
          width: 100%;
          text-align: left;
          padding: 8px 10px;
          border-radius: var(--radius);
          cursor: pointer;
        }
        .sp-item:hover,
        .sp-item.selected {
          background: var(--bg-hover);
        }
        .sp-item.selected {
          outline: 1px solid var(--accent);
        }
        .sp-item-main {
          display: flex;
          align-items: baseline;
          gap: 8px;
        }
        .sp-item-name {
          font-weight: 600;
          font-size: 13px;
          color: var(--text-primary);
        }
        .sp-item-desc {
          font-size: 12px;
          color: var(--text-secondary);
        }
        .sp-item-meta {
          display: flex;
          align-items: center;
          gap: 6px;
          margin-top: 2px;
        }
        .sp-cat-badge {
          font-size: 10px;
          padding: 1px 6px;
          border-radius: 8px;
          font-weight: 500;
          text-transform: uppercase;
        }
        .sp-cat-setup  { background: rgba(255, 152, 0, 0.15); color: var(--warning); }
        .sp-cat-build  { background: rgba(74, 158, 255, 0.15); color: var(--accent); }
        .sp-cat-test   { background: rgba(76, 175, 80, 0.15);  color: var(--success); }
        .sp-cat-lint   { background: rgba(156, 39, 176, 0.15); color: #ce93d8; }
        .sp-cat-deploy { background: rgba(244, 67, 54, 0.15);  color: var(--error); }
        .sp-cat-custom { background: rgba(255, 255, 255, 0.08); color: var(--text-secondary); }
        .sp-item-key {
          font-size: 10px;
          font-family: var(--font-mono);
          padding: 1px 5px;
          border-radius: 3px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          color: var(--text-muted);
        }
        .sp-item-cmd {
          font-size: 11px;
          font-family: var(--font-mono);
          color: var(--text-muted);
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
          max-width: 100%;
        }
      `}</style>
    </div>
  );
}
