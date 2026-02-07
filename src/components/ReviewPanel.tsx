import { createSignal, createEffect, onCleanup, Show } from "solid-js";
import type { WorkspaceTab } from "../store/workspace-store";
import type { WorktreeStatus, FileDiff, MergeResult, MergeStrategy } from "../lib/types";
import {
  getWorktreeStatus,
  getDiff,
  mergeWorkspace,
  discardWorkspace,
  onDiffReady,
  onMergeComplete,
} from "../lib/tauri";
import FileTree from "./review/FileTree";
import DiffViewer from "./review/DiffViewer";
import ReviewActions from "./review/ReviewActions";

interface Props {
  workspace: WorkspaceTab;
  width: number;
  onResize: (width: number) => void;
}

export default function ReviewPanel(props: Props) {
  const [tab, setTab] = createSignal<"files" | "changes" | "terminal">("files");
  const [status, setStatus] = createSignal<WorktreeStatus | null>(null);
  const [diffs, setDiffs] = createSignal<FileDiff[]>([]);
  const [selectedFile, setSelectedFile] = createSignal<string | null>(null);
  const [merging, setMerging] = createSignal(false);
  const [mergeResult, setMergeResult] = createSignal<MergeResult | null>(null);
  const [diffLoading, setDiffLoading] = createSignal(false);
  let resizing = false;

  // Load worktree status when workspace changes
  createEffect(() => {
    const ws = props.workspace;
    setStatus(null);
    setDiffs([]);
    setSelectedFile(null);
    setMergeResult(null);

    getWorktreeStatus(ws.id).then(setStatus).catch(() => {});

    const cleanups: (() => void)[] = [];

    onDiffReady(ws.id, (s) => {
      setStatus(s);
    }).then((unlisten) => cleanups.push(unlisten));

    onMergeComplete(ws.id, (result) => {
      setMerging(false);
      setMergeResult(result);
    }).then((unlisten) => cleanups.push(unlisten));

    onCleanup(() => {
      for (const fn of cleanups) fn();
    });
  });

  // Load diffs when a file is selected
  createEffect(() => {
    const path = selectedFile();
    const ws = props.workspace;
    if (!path) return;

    setDiffLoading(true);
    getDiff(ws.id, [path])
      .then(setDiffs)
      .catch(() => setDiffs([]))
      .finally(() => setDiffLoading(false));
  });

  function handleSelectFile(path: string) {
    setSelectedFile(path);
    setTab("changes");
  }

  async function handleMerge(strategy: MergeStrategy, message?: string) {
    setMerging(true);
    setMergeResult(null);
    try {
      const result = await mergeWorkspace(props.workspace.id, strategy, message);
      setMergeResult(result);
    } catch (err) {
      setMergeResult({
        success: false,
        merge_sha: null,
        conflicts: [],
        message: String(err),
      });
    } finally {
      setMerging(false);
    }
  }

  async function handleDiscard() {
    try {
      await discardWorkspace(props.workspace.id);
    } catch {
      // handled by backend
    }
  }

  function startResize(e: MouseEvent) {
    e.preventDefault();
    resizing = true;
    const startX = e.clientX;
    const startW = props.width;
    const onMove = (e: MouseEvent) => {
      if (!resizing) return;
      const delta = startX - e.clientX;
      const w = Math.max(250, Math.min(800, startW + delta));
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

  const fileCount = () => status()?.files_changed ?? 0;

  return (
    <>
      <div class="resize-handle" onMouseDown={startResize} />
      <div class="review-panel" style={{ width: `${props.width}px` }}>
        <div class="review-header">
          <div class="review-tabs">
            <button
              class="review-tab"
              classList={{ active: tab() === "files" }}
              onClick={() => setTab("files")}
            >
              Files
              <Show when={fileCount() > 0}>
                <span class="review-tab-badge">{fileCount()}</span>
              </Show>
            </button>
            <button
              class="review-tab"
              classList={{ active: tab() === "changes" }}
              onClick={() => setTab("changes")}
            >
              Changes
            </button>
            <button
              class="review-tab"
              classList={{ active: tab() === "terminal" }}
              onClick={() => setTab("terminal")}
            >
              Terminal
            </button>
          </div>
          <Show when={status()}>
            <div class="review-status-badge">
              <Show when={status()!.ahead > 0}>
                <span class="review-ahead">{status()!.ahead} ahead</span>
              </Show>
              <Show when={status()!.behind > 0}>
                <span class="review-behind">{status()!.behind} behind</span>
              </Show>
            </div>
          </Show>
        </div>

        <div class="review-content">
          <Show when={tab() === "files"}>
            <Show
              when={status() && status()!.files.length > 0}
              fallback={
                <div class="review-empty">No changes yet</div>
              }
            >
              <FileTree
                files={status()!.files}
                selectedPath={selectedFile()}
                onSelectFile={handleSelectFile}
              />
            </Show>
          </Show>

          <Show when={tab() === "changes"}>
            <Show when={diffLoading()}>
              <div class="review-empty">Loading diff...</div>
            </Show>
            <Show when={!diffLoading()}>
              <DiffViewer
                diffs={diffs()}
                selectedPath={selectedFile()}
              />
            </Show>
          </Show>

          <Show when={tab() === "terminal"}>
            <div class="review-terminal-placeholder">
              Terminal output will appear here
            </div>
          </Show>
        </div>

        <ReviewActions
          workspace={props.workspace}
          status={status()}
          onMerge={handleMerge}
          onDiscard={handleDiscard}
          merging={merging()}
          mergeResult={mergeResult()}
        />
      </div>

      <style>{`
        .review-panel {
          display: flex;
          flex-direction: column;
          background: var(--bg-secondary);
          border-left: 1px solid var(--border);
          flex-shrink: 0;
          overflow: hidden;
        }
        .review-header {
          height: var(--header-height);
          border-bottom: 1px solid var(--border);
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding-right: 8px;
        }
        .review-tabs {
          display: flex;
          gap: 0;
          height: 100%;
        }
        .review-tab {
          padding: 0 16px;
          font-size: 12px;
          color: var(--text-secondary);
          border-bottom: 2px solid transparent;
          height: 100%;
          display: flex;
          align-items: center;
          gap: 6px;
        }
        .review-tab:hover { color: var(--text-primary); }
        .review-tab.active {
          color: var(--text-primary);
          border-bottom-color: var(--accent);
        }
        .review-tab-badge {
          font-size: 10px;
          padding: 1px 5px;
          border-radius: 8px;
          background: var(--accent);
          color: #fff;
          font-weight: 600;
        }
        .review-status-badge {
          display: flex;
          gap: 8px;
          font-size: 11px;
        }
        .review-ahead { color: var(--success); }
        .review-behind { color: var(--warning); }
        .review-content {
          flex: 1;
          overflow-y: auto;
          display: flex;
          flex-direction: column;
        }
        .review-empty {
          display: flex;
          align-items: center;
          justify-content: center;
          height: 100%;
          color: var(--text-muted);
          font-size: 13px;
        }
        .review-terminal-placeholder {
          display: flex;
          align-items: center;
          justify-content: center;
          height: 100%;
          color: var(--text-muted);
        }
      `}</style>
    </>
  );
}
