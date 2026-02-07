import { For, onCleanup, onMount } from "solid-js";
import {
  state,
  setActiveWorkspace,
  closeWorkspace,
} from "../store/workspace-store";
import { destroyTerminal } from "../lib/terminal-cache";
import { registerAction, unregisterAction } from "../lib/action-registry";

function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max - 1) + "\u2026" : text;
}

function statusDotClass(
  tab: { status: string; terminalBuffer: { hasNewOutput: boolean } },
): string {
  if (tab.terminalBuffer.hasNewOutput) return "tab-dot has-new-output";
  switch (tab.status) {
    case "running":
      return "tab-dot running";
    case "stopped":
      return "tab-dot stopped";
    default:
      return "tab-dot idle";
  }
}

export default function TabBar() {
  function handleClose(id: string, status: string, e: MouseEvent) {
    e.stopPropagation();
    if (status === "running") {
      if (!confirm("This agent is still running. Close anyway?")) return;
    }
    destroyTerminal(id);
    closeWorkspace(id);
  }

  function handleMiddleClick(id: string, status: string, e: MouseEvent) {
    if (e.button === 1) {
      handleClose(id, status, e);
    }
  }

  function cycleTab(dir: 1 | -1) {
    if (state.tabs.length <= 1) return;
    const currentIdx = state.tabs.findIndex((t) => t.id === state.activeId);
    if (currentIdx === -1) return;
    const nextIdx =
      (currentIdx + dir + state.tabs.length) % state.tabs.length;
    setActiveWorkspace(state.tabs[nextIdx].id);
  }

  function jumpToTab(n: number) {
    const idx = n - 1;
    if (idx < state.tabs.length) {
      setActiveWorkspace(state.tabs[idx].id);
    }
  }

  function closeActiveTab() {
    const active = state.tabs.find((t) => t.id === state.activeId);
    if (active) {
      if (active.status === "running") {
        if (!confirm("This agent is still running. Close anyway?")) return;
      }
      destroyTerminal(active.id);
      closeWorkspace(active.id);
    }
  }

  onMount(() => {
    registerAction("tab.next", () => cycleTab(1));
    registerAction("tab.prev", () => cycleTab(-1));
    registerAction("workspace.close", closeActiveTab);
    for (let i = 1; i <= 9; i++) {
      registerAction(`tab.${i}`, () => jumpToTab(i));
    }
  });
  onCleanup(() => {
    unregisterAction("tab.next");
    unregisterAction("tab.prev");
    unregisterAction("workspace.close");
    for (let i = 1; i <= 9; i++) {
      unregisterAction(`tab.${i}`);
    }
  });

  return (
    <div class="tab-bar">
      <For each={state.tabs}>
        {(tab) => (
          <button
            class="tab"
            classList={{ active: tab.id === state.activeId }}
            onClick={() => setActiveWorkspace(tab.id)}
            onMouseDown={(e) => handleMiddleClick(tab.id, tab.status, e)}
            title={`${tab.repoAlias}/${tab.branch} - ${tab.agent}`}
          >
            <span class={statusDotClass(tab)} />
            <span class="tab-label">
              {truncate(`${tab.repoAlias}/${tab.branch}`, 28)}
            </span>
            <span class="tab-agent">{tab.agent}</span>
            <button
              class="tab-close"
              onClick={(e) => handleClose(tab.id, tab.status, e)}
            >
              &times;
            </button>
          </button>
        )}
      </For>

      <style>{`
        .tab-bar {
          display: flex;
          align-items: stretch;
          height: var(--header-height);
          background: var(--bg-secondary);
          border-bottom: 1px solid var(--border);
          overflow-x: auto;
          overflow-y: hidden;
          flex-shrink: 0;
        }
        .tab-bar::-webkit-scrollbar { height: 0; }
        .tab {
          display: flex;
          align-items: center;
          gap: 6px;
          padding: 0 12px;
          white-space: nowrap;
          font-size: 12px;
          border-right: 1px solid var(--border);
          color: var(--text-secondary);
          min-width: 0;
          flex-shrink: 0;
        }
        .tab:hover { background: var(--bg-hover); }
        .tab.active {
          background: var(--bg-primary);
          color: var(--text-primary);
          font-weight: 500;
        }
        .tab-dot {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          flex-shrink: 0;
        }
        .tab-dot.running {
          background: var(--success);
          animation: pulse 2s ease-in-out infinite;
        }
        .tab-dot.stopped { background: var(--error); }
        .tab-dot.idle { background: var(--text-muted); }
        .tab-dot.has-new-output {
          background: var(--accent);
          animation: flash 0.6s ease-out;
        }
        .tab-label {
          font-family: var(--font-mono);
          font-size: 11px;
        }
        .tab-agent {
          font-size: 10px;
          padding: 1px 6px;
          background: var(--bg-tertiary);
          border-radius: 8px;
          color: var(--text-muted);
        }
        .tab.active .tab-agent { color: var(--text-secondary); }
        .tab-close {
          margin-left: 4px;
          font-size: 14px;
          line-height: 1;
          color: var(--text-muted);
          opacity: 0;
          transition: opacity 0.1s;
        }
        .tab:hover .tab-close { opacity: 1; }
        .tab-close:hover { color: var(--error); }

        @keyframes pulse {
          0%, 100% { transform: scale(1); opacity: 1; }
          50% { transform: scale(1.4); opacity: 0.7; }
        }
        @keyframes flash {
          0% { opacity: 1; transform: scale(1.6); }
          100% { opacity: 1; transform: scale(1); }
        }
      `}</style>
    </div>
  );
}
