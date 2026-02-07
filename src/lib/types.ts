// ── Git Review Types ──

export type FileStatus = "added" | "modified" | "deleted" | "renamed" | "copied";

export interface FileChange {
  path: string;
  old_path: string | null;
  status: FileStatus;
  additions: number;
  deletions: number;
  binary: boolean;
}

export interface DiffLine {
  kind: "add" | "delete" | "context";
  old_lineno: number | null;
  new_lineno: number | null;
  content: string;
}

export interface DiffHunk {
  old_start: number;
  old_count: number;
  new_start: number;
  new_count: number;
  header: string;
  lines: DiffLine[];
}

export interface FileDiff {
  path: string;
  old_path: string | null;
  status: FileStatus;
  binary: boolean;
  hunks: DiffHunk[];
  additions: number;
  deletions: number;
}

export interface WorktreeStatus {
  workspace_id: string;
  base_branch: string;
  head_sha: string;
  base_sha: string;
  ahead: number;
  behind: number;
  files_changed: number;
  total_additions: number;
  total_deletions: number;
  files: FileChange[];
  has_conflicts: boolean;
  conflict_files: string[];
}

export interface MergeResult {
  success: boolean;
  merge_sha: string | null;
  conflicts: string[];
  message: string;
}

export type MergeStrategy = "merge" | "squash" | "rebase";
export type FileVersion = "base" | "working";

// ── Snippet Types ──

export type SnippetCategory = "setup" | "build" | "test" | "lint" | "deploy" | "custom";

export interface Snippet {
  name: string;
  command: string;
  description: string;
  category: SnippetCategory;
  keybinding: string | null;
}

// ── Trust Types ──

export type TrustStatus =
  | { type: "trusted" }
  | { type: "untrusted" }
  | { type: "changed"; changed_files: string[] };

export interface TrustEntry {
  trusted: boolean;
  trusted_at: string;
  setup_script_hash: string | null;
  snippets_hash: string | null;
}

export interface TrustRequiredPayload {
  repo_path: string;
  workspace_id: string;
  script_content: string;
  changed_files: string[];
}

// ── Shortcut Types ──

export interface ShortcutConfig {
  version: number;
  shortcuts: Record<string, string>;
}

export interface ShortcutBinding {
  action: string;
  binding: string;
  description: string;
}

export interface SecretKeyInfo {
  name: string;
  hasValue: boolean;
}
