import { createStore } from "solid-js/store";
import type { Repo } from "../App";

export interface TerminalBuffer {
  hasNewOutput: boolean;
  outputBytes: number;
}

export interface WorkspaceTab {
  id: string;
  repoPath: string;
  repoAlias: string;
  branch: string;
  agent: string;
  model: string | null;
  status: "idle" | "running" | "stopped";
  terminalBuffer: TerminalBuffer;
}

export interface WorkspaceState {
  tabs: WorkspaceTab[];
  activeId: string | null;
  repos: Repo[];
}

const [state, setState] = createStore<WorkspaceState>({
  tabs: [],
  activeId: null,
  repos: [],
});

export { state };

// ── Actions ──

export function openWorkspace(ws: {
  id: string;
  repoPath: string;
  repoAlias: string;
  branch: string;
  agent: string;
  status: "idle" | "running" | "stopped";
}): void {
  const exists = state.tabs.find((t) => t.id === ws.id);
  if (!exists) {
    setState("tabs", (tabs) => [
      ...tabs,
      {
        id: ws.id,
        repoPath: ws.repoPath,
        repoAlias: ws.repoAlias,
        branch: ws.branch,
        agent: ws.agent,
        model: null,
        status: ws.status,
        terminalBuffer: { hasNewOutput: false, outputBytes: 0 },
      },
    ]);
  }
  setState("activeId", ws.id);
  clearNewOutput(ws.id);
}

export function closeWorkspace(id: string): void {
  const idx = state.tabs.findIndex((t) => t.id === id);
  if (idx === -1) return;

  setState("tabs", (tabs) => tabs.filter((t) => t.id !== id));

  if (state.activeId === id) {
    // Select adjacent tab
    const remaining = state.tabs;
    if (remaining.length > 0) {
      const nextIdx = Math.min(idx, remaining.length - 1);
      setState("activeId", remaining[nextIdx].id);
    } else {
      setState("activeId", null);
    }
  }
}

export function setActiveWorkspace(id: string): void {
  setState("activeId", id);
  clearNewOutput(id);
}

export function reorderTabs(fromIndex: number, toIndex: number): void {
  setState("tabs", (tabs) => {
    const next = [...tabs];
    const [moved] = next.splice(fromIndex, 1);
    next.splice(toIndex, 0, moved);
    return next;
  });
}

export function updateWorkspaceStatus(
  id: string,
  status: WorkspaceTab["status"],
): void {
  setState(
    "tabs",
    (t) => t.id === id,
    "status",
    status,
  );
}

export function markNewOutput(id: string): void {
  if (state.activeId !== id) {
    setState(
      "tabs",
      (t) => t.id === id,
      "terminalBuffer",
      "hasNewOutput",
      true,
    );
  }
}

export function clearNewOutput(id: string): void {
  setState(
    "tabs",
    (t) => t.id === id,
    "terminalBuffer",
    "hasNewOutput",
    false,
  );
}

export function incrementOutputBytes(id: string, bytes: number): void {
  setState(
    "tabs",
    (t) => t.id === id,
    "terminalBuffer",
    "outputBytes",
    (prev) => prev + bytes,
  );
}

export function setRepos(repos: Repo[]): void {
  setState("repos", repos);
}

export function addRepo(repo: Repo): void {
  setState("repos", (prev) => [...prev, repo]);
}

export function setWorkspaceModel(id: string, model: string): void {
  setState("tabs", (t) => t.id === id, "model", model);
}

// ── Selectors ──

export function activeWorkspace(): WorkspaceTab | undefined {
  return state.tabs.find((t) => t.id === state.activeId);
}

export function activeTabs(): WorkspaceTab[] {
  return state.tabs;
}
