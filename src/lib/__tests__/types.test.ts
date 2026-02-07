import { describe, it, expect } from "vitest";
import type {
  FileStatus,
  FileChange,
  DiffLine,
  DiffHunk,
  FileDiff,
  WorktreeStatus,
  MergeResult,
  MergeStrategy,
  FileVersion,
  SnippetCategory,
  Snippet,
  TrustStatus,
  TrustEntry,
  TrustRequiredPayload,
  ShortcutConfig,
  ShortcutBinding,
  SecretKeyInfo,
} from "../types";

/**
 * These tests verify that type contracts are satisfied at compile time
 * and that objects conforming to the interfaces have the correct shape.
 * If the types change in incompatible ways, these tests will fail to compile.
 */

describe("types – Git Review Types", () => {
  describe("FileStatus", () => {
    it("accepts all valid status values", () => {
      const statuses: FileStatus[] = [
        "added",
        "modified",
        "deleted",
        "renamed",
        "copied",
      ];
      expect(statuses).toHaveLength(5);
      expect(statuses).toContain("added");
      expect(statuses).toContain("modified");
      expect(statuses).toContain("deleted");
      expect(statuses).toContain("renamed");
      expect(statuses).toContain("copied");
    });
  });

  describe("FileChange", () => {
    it("has required fields with correct types", () => {
      const change: FileChange = {
        path: "src/main.ts",
        old_path: null,
        status: "added",
        additions: 10,
        deletions: 0,
        binary: false,
      };
      expect(change.path).toBe("src/main.ts");
      expect(change.old_path).toBeNull();
      expect(change.status).toBe("added");
      expect(change.additions).toBe(10);
      expect(change.deletions).toBe(0);
      expect(change.binary).toBe(false);
    });

    it("allows non-null old_path for renamed files", () => {
      const change: FileChange = {
        path: "src/new-name.ts",
        old_path: "src/old-name.ts",
        status: "renamed",
        additions: 0,
        deletions: 0,
        binary: false,
      };
      expect(change.old_path).toBe("src/old-name.ts");
    });
  });

  describe("DiffLine", () => {
    it("accepts add kind with new_lineno", () => {
      const line: DiffLine = {
        kind: "add",
        old_lineno: null,
        new_lineno: 42,
        content: "+new line",
      };
      expect(line.kind).toBe("add");
      expect(line.old_lineno).toBeNull();
      expect(line.new_lineno).toBe(42);
    });

    it("accepts delete kind with old_lineno", () => {
      const line: DiffLine = {
        kind: "delete",
        old_lineno: 10,
        new_lineno: null,
        content: "-removed line",
      };
      expect(line.kind).toBe("delete");
      expect(line.old_lineno).toBe(10);
      expect(line.new_lineno).toBeNull();
    });

    it("accepts context kind with both line numbers", () => {
      const line: DiffLine = {
        kind: "context",
        old_lineno: 5,
        new_lineno: 5,
        content: " unchanged line",
      };
      expect(line.kind).toBe("context");
      expect(line.old_lineno).toBe(5);
      expect(line.new_lineno).toBe(5);
    });
  });

  describe("DiffHunk", () => {
    it("has all required fields including lines array", () => {
      const hunk: DiffHunk = {
        old_start: 1,
        old_count: 5,
        new_start: 1,
        new_count: 7,
        header: "@@ -1,5 +1,7 @@",
        lines: [
          { kind: "context", old_lineno: 1, new_lineno: 1, content: " line" },
        ],
      };
      expect(hunk.old_start).toBe(1);
      expect(hunk.old_count).toBe(5);
      expect(hunk.new_start).toBe(1);
      expect(hunk.new_count).toBe(7);
      expect(hunk.header).toContain("@@");
      expect(hunk.lines).toHaveLength(1);
    });
  });

  describe("FileDiff", () => {
    it("has all required fields", () => {
      const diff: FileDiff = {
        path: "src/index.ts",
        old_path: null,
        status: "modified",
        binary: false,
        hunks: [],
        additions: 3,
        deletions: 1,
      };
      expect(diff.path).toBe("src/index.ts");
      expect(diff.binary).toBe(false);
      expect(diff.hunks).toEqual([]);
      expect(diff.additions).toBe(3);
      expect(diff.deletions).toBe(1);
    });
  });

  describe("WorktreeStatus", () => {
    it("has all required fields", () => {
      const status: WorktreeStatus = {
        workspace_id: "ws-1",
        base_branch: "main",
        head_sha: "abc123",
        base_sha: "def456",
        ahead: 2,
        behind: 0,
        files_changed: 3,
        total_additions: 50,
        total_deletions: 10,
        files: [],
        has_conflicts: false,
        conflict_files: [],
      };
      expect(status.workspace_id).toBe("ws-1");
      expect(status.base_branch).toBe("main");
      expect(status.ahead).toBe(2);
      expect(status.behind).toBe(0);
      expect(status.has_conflicts).toBe(false);
      expect(status.conflict_files).toEqual([]);
    });

    it("supports conflict state", () => {
      const status: WorktreeStatus = {
        workspace_id: "ws-1",
        base_branch: "main",
        head_sha: "abc123",
        base_sha: "def456",
        ahead: 1,
        behind: 1,
        files_changed: 2,
        total_additions: 5,
        total_deletions: 5,
        files: [],
        has_conflicts: true,
        conflict_files: ["src/main.ts", "src/utils.ts"],
      };
      expect(status.has_conflicts).toBe(true);
      expect(status.conflict_files).toHaveLength(2);
    });
  });

  describe("MergeResult", () => {
    it("represents a successful merge", () => {
      const result: MergeResult = {
        success: true,
        merge_sha: "abc123def",
        conflicts: [],
        message: "Merged successfully",
      };
      expect(result.success).toBe(true);
      expect(result.merge_sha).toBe("abc123def");
      expect(result.conflicts).toHaveLength(0);
    });

    it("represents a failed merge with conflicts", () => {
      const result: MergeResult = {
        success: false,
        merge_sha: null,
        conflicts: ["src/main.ts"],
        message: "Merge conflict",
      };
      expect(result.success).toBe(false);
      expect(result.merge_sha).toBeNull();
      expect(result.conflicts).toHaveLength(1);
    });
  });

  describe("MergeStrategy", () => {
    it("accepts all valid values", () => {
      const strategies: MergeStrategy[] = ["merge", "squash", "rebase"];
      expect(strategies).toHaveLength(3);
    });
  });

  describe("FileVersion", () => {
    it("accepts all valid values", () => {
      const versions: FileVersion[] = ["base", "working"];
      expect(versions).toHaveLength(2);
    });
  });
});

describe("types – Snippet Types", () => {
  describe("SnippetCategory", () => {
    it("accepts all valid category values", () => {
      const categories: SnippetCategory[] = [
        "setup",
        "build",
        "test",
        "lint",
        "deploy",
        "custom",
      ];
      expect(categories).toHaveLength(6);
    });
  });

  describe("Snippet", () => {
    it("has all required fields", () => {
      const snippet: Snippet = {
        name: "Run Tests",
        command: "npm test",
        description: "Runs the test suite",
        category: "test",
        keybinding: "Cmd+Shift+T",
      };
      expect(snippet.name).toBe("Run Tests");
      expect(snippet.command).toBe("npm test");
      expect(snippet.description).toBe("Runs the test suite");
      expect(snippet.category).toBe("test");
      expect(snippet.keybinding).toBe("Cmd+Shift+T");
    });

    it("allows null keybinding", () => {
      const snippet: Snippet = {
        name: "Deploy",
        command: "npm run deploy",
        description: "Deploy to production",
        category: "deploy",
        keybinding: null,
      };
      expect(snippet.keybinding).toBeNull();
    });
  });
});

describe("types – Trust Types", () => {
  describe("TrustStatus discriminated union", () => {
    it("represents trusted state", () => {
      const status: TrustStatus = { type: "trusted" };
      expect(status.type).toBe("trusted");
    });

    it("represents untrusted state", () => {
      const status: TrustStatus = { type: "untrusted" };
      expect(status.type).toBe("untrusted");
    });

    it("represents changed state with changed_files", () => {
      const status: TrustStatus = {
        type: "changed",
        changed_files: ["setup.sh", "config.json"],
      };
      expect(status.type).toBe("changed");
      if (status.type === "changed") {
        expect(status.changed_files).toHaveLength(2);
        expect(status.changed_files).toContain("setup.sh");
        expect(status.changed_files).toContain("config.json");
      }
    });

    it("can be discriminated in switch statements", () => {
      const statuses: TrustStatus[] = [
        { type: "trusted" },
        { type: "untrusted" },
        { type: "changed", changed_files: ["a.txt"] },
      ];

      const results: string[] = [];
      for (const s of statuses) {
        switch (s.type) {
          case "trusted":
            results.push("trusted");
            break;
          case "untrusted":
            results.push("untrusted");
            break;
          case "changed":
            results.push(`changed:${s.changed_files.length}`);
            break;
        }
      }

      expect(results).toEqual(["trusted", "untrusted", "changed:1"]);
    });
  });

  describe("TrustEntry", () => {
    it("has all required fields", () => {
      const entry: TrustEntry = {
        trusted: true,
        trusted_at: "2025-01-15T10:00:00Z",
        setup_script_hash: "sha256:abc123",
        snippets_hash: null,
      };
      expect(entry.trusted).toBe(true);
      expect(entry.trusted_at).toBe("2025-01-15T10:00:00Z");
      expect(entry.setup_script_hash).toBe("sha256:abc123");
      expect(entry.snippets_hash).toBeNull();
    });
  });

  describe("TrustRequiredPayload", () => {
    it("has all required fields", () => {
      const payload: TrustRequiredPayload = {
        repo_path: "/home/user/my-repo",
        workspace_id: "ws-123",
        script_content: "#!/bin/bash\nnpm install",
        changed_files: ["setup.sh"],
      };
      expect(payload.repo_path).toBe("/home/user/my-repo");
      expect(payload.workspace_id).toBe("ws-123");
      expect(payload.script_content).toContain("npm install");
      expect(payload.changed_files).toHaveLength(1);
    });
  });
});

describe("types – Shortcut Types", () => {
  describe("ShortcutConfig", () => {
    it("has version and shortcuts record", () => {
      const config: ShortcutConfig = {
        version: 1,
        shortcuts: {
          "workspace.new": "Cmd+N",
          "settings.open": "Cmd+,",
        },
      };
      expect(config.version).toBe(1);
      expect(config.shortcuts["workspace.new"]).toBe("Cmd+N");
      expect(config.shortcuts["settings.open"]).toBe("Cmd+,");
    });

    it("allows empty shortcuts record", () => {
      const config: ShortcutConfig = {
        version: 1,
        shortcuts: {},
      };
      expect(config.version).toBe(1);
      expect(Object.keys(config.shortcuts)).toHaveLength(0);
    });
  });

  describe("ShortcutBinding", () => {
    it("has action, binding, and description", () => {
      const binding: ShortcutBinding = {
        action: "workspace.new",
        binding: "Cmd+N",
        description: "Create a new workspace",
      };
      expect(binding.action).toBe("workspace.new");
      expect(binding.binding).toBe("Cmd+N");
      expect(binding.description).toBe("Create a new workspace");
    });
  });

  describe("SecretKeyInfo", () => {
    it("has name and hasValue fields", () => {
      const key: SecretKeyInfo = {
        name: "OPENAI_API_KEY",
        hasValue: true,
      };
      expect(key.name).toBe("OPENAI_API_KEY");
      expect(key.hasValue).toBe(true);
    });

    it("represents a key without value", () => {
      const key: SecretKeyInfo = {
        name: "ANTHROPIC_API_KEY",
        hasValue: false,
      };
      expect(key.name).toBe("ANTHROPIC_API_KEY");
      expect(key.hasValue).toBe(false);
    });
  });
});
