import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  WorktreeStatus,
  FileDiff,
  FileVersion,
  MergeStrategy,
  MergeResult,
  Snippet,
  TrustStatus,
  TrustRequiredPayload,
  ShortcutConfig,
} from "./types";

// ── Config ──

export async function getConfig() {
  return invoke<Record<string, unknown>>("get_config");
}

export async function saveConfig(global: Record<string, unknown>) {
  return invoke("save_config", { global });
}

// ── Repositories ──

export interface RepoRef {
  path: string;
  alias: string;
}

export async function addRepository(path: string, alias: string) {
  return invoke("add_repository", { path, alias });
}

export async function removeRepository(path: string) {
  return invoke("remove_repository", { path });
}

export async function listRepositories(): Promise<RepoRef[]> {
  return invoke("list_repositories");
}

// ── Workspaces ──

export interface WorkspaceInfo {
  id: string;
  repo_path: string;
  repo_alias: string;
  branch: string;
  worktree_path: string;
  agent: string;
  state: "Creating" | "Running" | "Stopping" | "Stopped" | "Cleaning";
}

export async function createWorkspace(args: {
  repo_path: string;
  repo_alias: string;
  branch: string;
  agent?: string;
}): Promise<WorkspaceInfo> {
  return invoke("create_workspace", { args });
}

export async function stopWorkspace(workspaceId: string) {
  return invoke("stop_workspace", { workspaceId });
}

export async function removeWorkspace(workspaceId: string) {
  return invoke("remove_workspace", { workspaceId });
}

export async function listWorkspaces(
  repoPath?: string,
): Promise<WorkspaceInfo[]> {
  return invoke("list_workspaces", { repoPath });
}

// ── Agent ──

export async function sendToAgent(workspaceId: string, message: string) {
  return invoke("send_to_agent", { workspaceId, message });
}

export async function getAgents(): Promise<string[]> {
  return invoke("get_agents");
}

// ── PTY Events ──

export function onPtyOutput(
  workspaceId: string,
  callback: (data: string) => void,
): Promise<UnlistenFn> {
  return listen<string>(`pty-output-${workspaceId}`, (event) => {
    callback(event.payload);
  });
}

export function onPtyExit(
  workspaceId: string,
  callback: () => void,
): Promise<UnlistenFn> {
  return listen(`pty-exit-${workspaceId}`, () => {
    callback();
  });
}

// ── Snippets ──

export async function runSnippet(
  workspaceId: string,
  snippetName: string,
): Promise<string> {
  return invoke("run_snippet", { workspaceId, snippetName });
}

// ── Snippets V2 ──

export async function listSnippets(repoPath?: string): Promise<Snippet[]> {
  return invoke("list_snippets", { repoPath });
}

export async function saveSnippet(repoPath: string | null, snippet: Snippet): Promise<void> {
  return invoke("save_snippet", { repoPath, snippet });
}

export async function deleteSnippet(repoPath: string | null, name: string): Promise<void> {
  return invoke("delete_snippet", { repoPath, name });
}

export async function runSnippetV2(workspaceId: string, name: string): Promise<void> {
  return invoke("run_snippet_v2", { workspaceId, name });
}

export function onSnippetOutput(
  workspaceId: string,
  callback: (data: string) => void,
): Promise<UnlistenFn> {
  return listen<string>(`snippet-output-${workspaceId}`, (event) => {
    callback(event.payload);
  });
}

export function onSnippetComplete(
  workspaceId: string,
  callback: (exitCode: number) => void,
): Promise<UnlistenFn> {
  return listen<number>(`snippet-complete-${workspaceId}`, (event) => {
    callback(event.payload);
  });
}

// ── Trust ──

export async function checkTrust(repoPath: string): Promise<TrustStatus> {
  return invoke("check_trust", { repoPath });
}

export async function grantTrust(repoPath: string): Promise<void> {
  return invoke("grant_trust", { repoPath });
}

export async function revokeTrust(repoPath: string): Promise<void> {
  return invoke("revoke_trust", { repoPath });
}

export function onTrustRequired(
  callback: (payload: TrustRequiredPayload) => void,
): Promise<UnlistenFn> {
  return listen<TrustRequiredPayload>("trust-required", (event) => {
    callback(event.payload);
  });
}

// ── Git Review ──

export async function getWorktreeStatus(
  workspaceId: string,
): Promise<WorktreeStatus> {
  return invoke("get_worktree_status", { workspaceId });
}

export async function getDiff(
  workspaceId: string,
  paths?: string[],
): Promise<FileDiff[]> {
  return invoke("get_diff", { workspaceId, paths });
}

export async function getFileContent(
  workspaceId: string,
  path: string,
  version: FileVersion,
): Promise<string> {
  return invoke("get_file_content", { workspaceId, path, version });
}

export async function mergeWorkspace(
  workspaceId: string,
  strategy: MergeStrategy,
  commitMessage?: string,
): Promise<MergeResult> {
  return invoke("merge_workspace", { workspaceId, strategy, commitMessage });
}

export async function discardWorkspace(workspaceId: string): Promise<void> {
  return invoke("discard_workspace", { workspaceId });
}

// ── Git Review Events ──

export function onDiffReady(
  workspaceId: string,
  callback: (status: WorktreeStatus) => void,
): Promise<UnlistenFn> {
  return listen<WorktreeStatus>(`diff-ready-${workspaceId}`, (event) => {
    callback(event.payload);
  });
}

export function onMergeComplete(
  workspaceId: string,
  callback: (result: MergeResult) => void,
): Promise<UnlistenFn> {
  return listen<MergeResult>(`merge-complete-${workspaceId}`, (event) => {
    callback(event.payload);
  });
}

// ── Shortcuts ──

export async function listShortcuts(): Promise<ShortcutConfig> {
  return invoke("list_shortcuts");
}

export async function saveShortcuts(shortcuts: ShortcutConfig): Promise<void> {
  return invoke("save_shortcuts", { config: shortcuts });
}

export function onShortcutsUpdated(
  callback: (config: ShortcutConfig) => void,
): Promise<UnlistenFn> {
  return listen<ShortcutConfig>("shortcuts-updated", (event) => {
    callback(event.payload);
  });
}

// ── Secrets ──

export async function getSecret(key: string): Promise<string> {
  return invoke("get_secret", { key });
}

export async function setSecret(key: string, value: string): Promise<void> {
  return invoke("set_secret", { key, value });
}

export async function deleteSecret(key: string): Promise<void> {
  return invoke("delete_secret", { key });
}

export async function listSecretKeys(): Promise<string[]> {
  return invoke("list_secret_keys");
}

// ── Model Switching ──

export async function switchAgentModel(
  workspaceId: string,
  model: string,
): Promise<void> {
  return invoke("switch_agent_model", { workspaceId, model });
}
