import { describe, it, expect, vi, beforeEach } from "vitest";

// We need to mock solid-js/store before importing workspace-store,
// because workspace-store calls createStore at module scope.
// The mock provides a functional createStore that behaves like a
// plain mutable object (no reactivity, no tracking).

vi.mock("solid-js/store", () => {
  return {
    createStore: vi.fn(<T extends object>(initial: T): [T, (...args: any[]) => void] => {
      const data = structuredClone(initial) as any;

      const setState = (...args: any[]) => {
        // Handle the various SolidJS setState overload patterns:
        // setState("key", value)
        // setState("key", updaterFn)
        // setState("key", filterFn, "nestedKey", value)
        // setState("key", filterFn, "nestedKey", "deepKey", value)
        // setState("key", filterFn, "nestedKey", "deepKey", updaterFn)

        if (args.length === 2) {
          const [key, valueOrFn] = args;
          if (typeof valueOrFn === "function") {
            data[key] = valueOrFn(data[key]);
          } else {
            data[key] = valueOrFn;
          }
        } else if (args.length === 4) {
          // setState("tabs", filterFn, "nestedKey", value)
          const [key, filterFn, nestedKey, value] = args;
          if (typeof filterFn === "function") {
            for (const item of data[key]) {
              if (filterFn(item)) {
                item[nestedKey] = value;
              }
            }
          }
        } else if (args.length === 5) {
          // setState("tabs", filterFn, "nestedKey", "deepKey", valueOrFn)
          const [key, filterFn, nestedKey, deepKey, valueOrFn] = args;
          if (typeof filterFn === "function") {
            for (const item of data[key]) {
              if (filterFn(item)) {
                if (typeof valueOrFn === "function") {
                  item[nestedKey][deepKey] = valueOrFn(item[nestedKey][deepKey]);
                } else {
                  item[nestedKey][deepKey] = valueOrFn;
                }
              }
            }
          }
        }
      };

      return [data, setState];
    }),
  };
});

import {
  state,
  openWorkspace,
  closeWorkspace,
  setActiveWorkspace,
  reorderTabs,
  updateWorkspaceStatus,
  markNewOutput,
  clearNewOutput,
  incrementOutputBytes,
  setRepos,
  addRepo,
  setWorkspaceModel,
  activeWorkspace,
  activeTabs,
} from "../../store/workspace-store";

function makeWorkspace(overrides: Partial<{
  id: string;
  repoPath: string;
  repoAlias: string;
  branch: string;
  agent: string;
  status: "idle" | "running" | "stopped";
}> = {}) {
  return {
    id: overrides.id ?? "ws-1",
    repoPath: overrides.repoPath ?? "/path/to/repo",
    repoAlias: overrides.repoAlias ?? "my-repo",
    branch: overrides.branch ?? "main",
    agent: overrides.agent ?? "claude",
    status: overrides.status ?? "idle" as const,
  };
}

describe("workspace-store", () => {
  beforeEach(() => {
    // Reset state between tests
    // Remove all tabs and reset activeId
    state.tabs.length = 0;
    state.activeId = null;
    (state as any).repos = [];
  });

  describe("state", () => {
    it("has initial shape with tabs, activeId, and repos", () => {
      expect(state).toHaveProperty("tabs");
      expect(state).toHaveProperty("activeId");
      expect(state).toHaveProperty("repos");
      expect(Array.isArray(state.tabs)).toBe(true);
      expect(Array.isArray(state.repos)).toBe(true);
    });
  });

  describe("openWorkspace", () => {
    it("adds a new tab to state", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      expect(state.tabs).toHaveLength(1);
      expect(state.tabs[0].id).toBe("ws-1");
    });

    it("sets activeId to the opened workspace", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      expect(state.activeId).toBe("ws-1");
    });

    it("does not duplicate a workspace that already exists", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      expect(state.tabs).toHaveLength(1);
    });

    it("sets model to null and initializes terminalBuffer", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      expect(state.tabs[0].model).toBeNull();
      expect(state.tabs[0].terminalBuffer).toEqual({
        hasNewOutput: false,
        outputBytes: 0,
      });
    });

    it("can open multiple distinct workspaces", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      openWorkspace(makeWorkspace({ id: "ws-2" }));
      expect(state.tabs).toHaveLength(2);
      // Last opened becomes active
      expect(state.activeId).toBe("ws-2");
    });

    it("preserves workspace fields correctly", () => {
      openWorkspace(makeWorkspace({
        id: "ws-1",
        repoPath: "/custom/path",
        repoAlias: "custom-repo",
        branch: "feature",
        agent: "gpt",
        status: "running",
      }));
      const tab = state.tabs[0];
      expect(tab.repoPath).toBe("/custom/path");
      expect(tab.repoAlias).toBe("custom-repo");
      expect(tab.branch).toBe("feature");
      expect(tab.agent).toBe("gpt");
      expect(tab.status).toBe("running");
    });
  });

  describe("closeWorkspace", () => {
    it("removes the tab from state", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      closeWorkspace("ws-1");
      expect(state.tabs).toHaveLength(0);
    });

    it("does not crash when closing non-existent workspace", () => {
      expect(() => closeWorkspace("nonexistent")).not.toThrow();
    });

    it("sets activeId to null when last tab is closed", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      closeWorkspace("ws-1");
      expect(state.activeId).toBeNull();
    });

    it("selects an adjacent tab when active tab is closed", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      openWorkspace(makeWorkspace({ id: "ws-2" }));
      openWorkspace(makeWorkspace({ id: "ws-3" }));
      // Active is ws-3 (last opened)
      setActiveWorkspace("ws-2");
      closeWorkspace("ws-2");
      // Should select adjacent tab
      expect(state.activeId).not.toBe("ws-2");
      expect(state.tabs).toHaveLength(2);
    });

    it("does not change activeId when a non-active tab is closed", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      openWorkspace(makeWorkspace({ id: "ws-2" }));
      // Active is ws-2
      closeWorkspace("ws-1");
      expect(state.activeId).toBe("ws-2");
      expect(state.tabs).toHaveLength(1);
    });
  });

  describe("setActiveWorkspace", () => {
    it("updates the activeId", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      openWorkspace(makeWorkspace({ id: "ws-2" }));
      setActiveWorkspace("ws-1");
      expect(state.activeId).toBe("ws-1");
    });
  });

  describe("reorderTabs", () => {
    it("moves a tab from one position to another", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      openWorkspace(makeWorkspace({ id: "ws-2" }));
      openWorkspace(makeWorkspace({ id: "ws-3" }));
      // Order: ws-1, ws-2, ws-3
      reorderTabs(0, 2);
      // After moving index 0 to index 2: ws-2, ws-3, ws-1
      expect(state.tabs[0].id).toBe("ws-2");
      expect(state.tabs[2].id).toBe("ws-1");
    });

    it("does not crash with same from and to index", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      expect(() => reorderTabs(0, 0)).not.toThrow();
    });
  });

  describe("updateWorkspaceStatus", () => {
    it("updates the status of a specific workspace", () => {
      openWorkspace(makeWorkspace({ id: "ws-1", status: "idle" }));
      updateWorkspaceStatus("ws-1", "running");
      expect(state.tabs[0].status).toBe("running");
    });

    it("can set status to stopped", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      updateWorkspaceStatus("ws-1", "stopped");
      expect(state.tabs[0].status).toBe("stopped");
    });
  });

  describe("markNewOutput / clearNewOutput", () => {
    it("marks new output for inactive workspace", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      openWorkspace(makeWorkspace({ id: "ws-2" }));
      // Active is ws-2, so marking ws-1 should set hasNewOutput
      markNewOutput("ws-1");
      expect(state.tabs[0].terminalBuffer.hasNewOutput).toBe(true);
    });

    it("does not mark new output for active workspace", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      // ws-1 is active
      markNewOutput("ws-1");
      expect(state.tabs[0].terminalBuffer.hasNewOutput).toBe(false);
    });

    it("clearNewOutput resets hasNewOutput to false", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      openWorkspace(makeWorkspace({ id: "ws-2" }));
      markNewOutput("ws-1");
      expect(state.tabs[0].terminalBuffer.hasNewOutput).toBe(true);
      clearNewOutput("ws-1");
      expect(state.tabs[0].terminalBuffer.hasNewOutput).toBe(false);
    });
  });

  describe("incrementOutputBytes", () => {
    it("adds bytes to the terminal buffer count", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      incrementOutputBytes("ws-1", 100);
      expect(state.tabs[0].terminalBuffer.outputBytes).toBe(100);
    });

    it("accumulates bytes across multiple calls", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      incrementOutputBytes("ws-1", 50);
      incrementOutputBytes("ws-1", 75);
      expect(state.tabs[0].terminalBuffer.outputBytes).toBe(125);
    });
  });

  describe("setRepos", () => {
    it("replaces the repos array", () => {
      const repos = [
        { path: "/repo1", alias: "r1", workspaces: [] },
        { path: "/repo2", alias: "r2", workspaces: [] },
      ];
      setRepos(repos);
      expect(state.repos).toHaveLength(2);
      expect(state.repos[0].path).toBe("/repo1");
    });
  });

  describe("addRepo", () => {
    it("appends a repo to the list", () => {
      setRepos([]);
      addRepo({ path: "/repo1", alias: "r1", workspaces: [] });
      expect(state.repos).toHaveLength(1);
      expect(state.repos[0].alias).toBe("r1");
    });

    it("appends without removing existing repos", () => {
      setRepos([{ path: "/repo1", alias: "r1", workspaces: [] }]);
      addRepo({ path: "/repo2", alias: "r2", workspaces: [] });
      expect(state.repos).toHaveLength(2);
    });
  });

  describe("setWorkspaceModel", () => {
    it("sets the model for a specific workspace", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      setWorkspaceModel("ws-1", "claude-3-opus");
      expect(state.tabs[0].model).toBe("claude-3-opus");
    });
  });

  describe("activeWorkspace selector", () => {
    it("returns undefined when no workspace is active", () => {
      expect(activeWorkspace()).toBeUndefined();
    });

    it("returns the active workspace tab", () => {
      openWorkspace(makeWorkspace({ id: "ws-1", repoAlias: "my-repo" }));
      const active = activeWorkspace();
      expect(active).toBeDefined();
      expect(active!.id).toBe("ws-1");
      expect(active!.repoAlias).toBe("my-repo");
    });
  });

  describe("activeTabs selector", () => {
    it("returns empty array when no tabs exist", () => {
      expect(activeTabs()).toEqual([]);
    });

    it("returns all tabs", () => {
      openWorkspace(makeWorkspace({ id: "ws-1" }));
      openWorkspace(makeWorkspace({ id: "ws-2" }));
      const tabs = activeTabs();
      expect(tabs).toHaveLength(2);
    });
  });
});
