import { For, createSignal, Show } from "solid-js";
import {
  state,
  openWorkspace,
  addRepo,
} from "../store/workspace-store";
import { stopWorkspace, removeWorkspace } from "../lib/tauri";
import { destroyTerminal } from "../lib/terminal-cache";
import { closeWorkspace } from "../store/workspace-store";
import type { Repo, Workspace } from "../App";

interface Props {
  width: number;
  onResize: (width: number) => void;
}

export default function Sidebar(props: Props) {
  const [expandedRepos, setExpandedRepos] = createSignal<Set<string>>(
    new Set(),
  );
  const [contextMenu, setContextMenu] = createSignal<{
    x: number;
    y: number;
    workspaceId: string;
    status: string;
  } | null>(null);
  let resizing = false;

  function toggleRepo(path: string) {
    setExpandedRepos((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }

  function startResize(e: MouseEvent) {
    e.preventDefault();
    resizing = true;
    const onMove = (e: MouseEvent) => {
      if (!resizing) return;
      const w = Math.max(180, Math.min(500, e.clientX));
      props.onResize(w);
    };
    const onUp = () => {
      resizing = false;
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  function statusColor(status: Workspace["status"]) {
    switch (status) {
      case "running":
        return "var(--success)";
      case "stopped":
        return "var(--error)";
      default:
        return "var(--text-muted)";
    }
  }

  function statusDotClass(ws: Workspace): string {
    switch (ws.status) {
      case "running":
        return "ws-status pulse";
      case "stopped":
        return "ws-status";
      default:
        return "ws-status";
    }
  }

  function handleSelectWorkspace(ws: Workspace) {
    openWorkspace({
      id: ws.id,
      repoPath: ws.repoPath,
      repoAlias: ws.repoAlias,
      branch: ws.branch,
      agent: ws.agent,
      status: ws.status,
    });
  }

  function handleContextMenu(ws: Workspace, e: MouseEvent) {
    e.preventDefault();
    setContextMenu({
      x: e.clientX,
      y: e.clientY,
      workspaceId: ws.id,
      status: ws.status,
    });
  }

  function closeContextMenu() {
    setContextMenu(null);
  }

  async function handleStop(id: string) {
    closeContextMenu();
    try {
      await stopWorkspace(id);
    } catch {
      // May fail if already stopped
    }
  }

  async function handleRemove(id: string) {
    closeContextMenu();
    try {
      await removeWorkspace(id);
      destroyTerminal(id);
      closeWorkspace(id);
    } catch {
      // May fail
    }
  }

  function handleCopyPath(id: string) {
    closeContextMenu();
    // Find workspace in repos to get worktree path
    for (const repo of state.repos) {
      const ws = repo.workspaces.find((w) => w.id === id);
      if (ws) {
        navigator.clipboard.writeText(ws.repoPath);
        break;
      }
    }
  }

  async function handleAddRepo() {
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      // TODO: Use Tauri dialog to pick directory
    } catch {
      // Fallback: prompt for path
      const path = window.prompt("Repository path:");
      if (!path) return;
      const alias = path.split("/").pop() || path;
      addRepo({ path, alias, workspaces: [] });
    }
  }

  function handleAddWorkspace(repo: Repo) {
    // TODO: Show workspace creation dialog with branch/agent selector
    // For now, create a simple workspace entry
    const branch = window.prompt("Branch name:", "main");
    if (!branch) return;
    const id = `${repo.alias}-${branch}-${Date.now()}`;
    openWorkspace({
      id,
      repoPath: repo.path,
      repoAlias: repo.alias,
      branch,
      agent: "claude",
      status: "idle",
    });
  }

  return (
    <>
      <div class="sidebar" style={{ width: `${props.width}px` }}>
        <div class="sidebar-header">
          <span class="sidebar-title">Workspaces</span>
          <button class="sidebar-add" onClick={handleAddRepo} title="Add repository">
            +
          </button>
        </div>
        <div class="sidebar-content" onClick={closeContextMenu}>
          <For each={state.repos}>
            {(repo) => (
              <div class="repo-group">
                <button
                  class="repo-header"
                  onClick={() => toggleRepo(repo.path)}
                >
                  <span class="repo-chevron">
                    {expandedRepos().has(repo.path) ? "\u25BE" : "\u25B8"}
                  </span>
                  <span class="repo-alias">{repo.alias}</span>
                  <span class="repo-count">{repo.workspaces.length}</span>
                  <button
                    class="repo-add-ws"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleAddWorkspace(repo);
                    }}
                    title="New workspace"
                  >
                    +
                  </button>
                </button>
                <Show when={expandedRepos().has(repo.path)}>
                  <div class="workspace-list">
                    <For each={repo.workspaces}>
                      {(ws) => (
                        <button
                          class="workspace-item"
                          classList={{
                            active: state.activeId === ws.id,
                          }}
                          onClick={() => handleSelectWorkspace(ws)}
                          onContextMenu={(e) => handleContextMenu(ws, e)}
                        >
                          <span
                            class={statusDotClass(ws)}
                            style={{ background: statusColor(ws.status) }}
                          />
                          <span class="ws-branch">{ws.branch}</span>
                          <span class="ws-agent">{ws.agent}</span>
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            )}
          </For>
          <Show when={state.repos.length === 0}>
            <div class="sidebar-empty">
              <p>No repositories</p>
              <button class="btn-add-repo" onClick={handleAddRepo}>
                + Add Repository
              </button>
            </div>
          </Show>
        </div>
      </div>
      <div class="resize-handle" onMouseDown={startResize} />

      {/* Context Menu */}
      <Show when={contextMenu()}>
        {(menu) => (
          <div
            class="context-menu-overlay"
            onClick={closeContextMenu}
            onContextMenu={(e) => { e.preventDefault(); closeContextMenu(); }}
          >
            <div
              class="context-menu"
              style={{ left: `${menu().x}px`, top: `${menu().y}px` }}
              onClick={(e) => e.stopPropagation()}
            >
              <button
                class="context-item"
                disabled={menu().status !== "running"}
                onClick={() => handleStop(menu().workspaceId)}
              >
                Stop Agent
              </button>
              <button
                class="context-item"
                disabled={menu().status === "running"}
                onClick={() => handleRemove(menu().workspaceId)}
              >
                Remove Workspace
              </button>
              <button
                class="context-item"
                onClick={() => handleCopyPath(menu().workspaceId)}
              >
                Copy Path
              </button>
            </div>
          </div>
        )}
      </Show>

      <style>{`
        .sidebar {
          display: flex;
          flex-direction: column;
          background: var(--bg-secondary);
          border-right: 1px solid var(--border);
          flex-shrink: 0;
          overflow: hidden;
        }
        .sidebar-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: 10px 14px;
          height: var(--header-height);
          border-bottom: 1px solid var(--border);
        }
        .sidebar-title {
          font-weight: 600;
          font-size: 12px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-secondary);
        }
        .sidebar-add {
          width: 22px;
          height: 22px;
          display: flex;
          align-items: center;
          justify-content: center;
          border-radius: var(--radius);
          font-size: 16px;
          color: var(--text-secondary);
        }
        .sidebar-add:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
        .sidebar-content {
          flex: 1;
          overflow-y: auto;
          padding: 6px 0;
        }
        .repo-header {
          display: flex;
          align-items: center;
          gap: 6px;
          width: 100%;
          padding: 6px 14px;
          text-align: left;
          font-size: 13px;
          font-weight: 500;
        }
        .repo-header:hover { background: var(--bg-hover); }
        .repo-chevron { color: var(--text-muted); font-size: 10px; width: 12px; }
        .repo-alias { flex: 1; }
        .repo-count {
          font-size: 11px;
          color: var(--text-muted);
          background: var(--bg-tertiary);
          padding: 1px 6px;
          border-radius: 10px;
        }
        .repo-add-ws {
          width: 18px;
          height: 18px;
          display: flex;
          align-items: center;
          justify-content: center;
          border-radius: var(--radius);
          font-size: 14px;
          color: var(--text-muted);
          opacity: 0;
          transition: opacity 0.1s;
        }
        .repo-header:hover .repo-add-ws { opacity: 1; }
        .repo-add-ws:hover {
          background: var(--bg-tertiary);
          color: var(--text-primary);
        }
        .workspace-list { padding-left: 8px; }
        .workspace-item {
          display: flex;
          align-items: center;
          gap: 8px;
          width: 100%;
          padding: 5px 14px 5px 28px;
          text-align: left;
          font-size: 12px;
          border-radius: 4px;
          margin: 1px 4px;
        }
        .workspace-item:hover { background: var(--bg-hover); }
        .workspace-item.active { background: var(--accent); color: white; }
        .ws-status {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          flex-shrink: 0;
        }
        .ws-status.pulse {
          animation: sidebar-pulse 2s ease-in-out infinite;
        }
        .ws-branch { flex: 1; font-family: var(--font-mono); }
        .ws-agent { font-size: 11px; color: var(--text-muted); }
        .workspace-item.active .ws-agent { color: rgba(255,255,255,0.7); }
        .sidebar-empty {
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 12px;
          padding: 40px 20px;
          color: var(--text-muted);
          font-size: 13px;
        }
        .btn-add-repo {
          padding: 6px 16px;
          background: var(--accent);
          color: white;
          border-radius: var(--radius);
          font-size: 12px;
          font-weight: 500;
        }
        .btn-add-repo:hover { background: var(--accent-hover); }

        /* Context Menu */
        .context-menu-overlay {
          position: fixed;
          inset: 0;
          z-index: 1000;
        }
        .context-menu {
          position: fixed;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: var(--radius);
          padding: 4px 0;
          min-width: 160px;
          box-shadow: 0 4px 12px rgba(0,0,0,0.3);
          z-index: 1001;
        }
        .context-item {
          display: block;
          width: 100%;
          padding: 6px 14px;
          text-align: left;
          font-size: 12px;
          color: var(--text-primary);
        }
        .context-item:hover:not(:disabled) { background: var(--bg-hover); }
        .context-item:disabled {
          color: var(--text-muted);
          cursor: default;
        }

        @keyframes sidebar-pulse {
          0%, 100% { transform: scale(1); opacity: 1; }
          50% { transform: scale(1.4); opacity: 0.7; }
        }
      `}</style>
    </>
  );
}
