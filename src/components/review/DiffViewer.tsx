import { createEffect, createSignal, For, Show, onMount } from "solid-js";
import type { FileDiff, DiffHunk, DiffLine } from "../../lib/types";

interface DiffViewerProps {
  diffs: FileDiff[];
  selectedPath: string | null;
}

function statusLabel(status: string): string {
  switch (status) {
    case "added": return "Added";
    case "modified": return "Modified";
    case "deleted": return "Deleted";
    case "renamed": return "Renamed";
    case "copied": return "Copied";
    default: return status;
  }
}

function HunkView(props: { hunk: DiffHunk }) {
  return (
    <div class="dv-hunk">
      <div class="dv-hunk-header">{props.hunk.header}</div>
      <For each={props.hunk.lines}>
        {(line) => <LineView line={line} />}
      </For>
    </div>
  );
}

function LineView(props: { line: DiffLine }) {
  const lineClass = () => {
    switch (props.line.kind) {
      case "add": return "dv-line dv-line-add";
      case "delete": return "dv-line dv-line-del";
      default: return "dv-line";
    }
  };

  const prefix = () => {
    switch (props.line.kind) {
      case "add": return "+";
      case "delete": return "-";
      default: return " ";
    }
  };

  return (
    <div class={lineClass()}>
      <span class="dv-lineno dv-lineno-old">
        {props.line.old_lineno ?? ""}
      </span>
      <span class="dv-lineno dv-lineno-new">
        {props.line.new_lineno ?? ""}
      </span>
      <span class="dv-prefix">{prefix()}</span>
      <span class="dv-content">{props.line.content}</span>
    </div>
  );
}

export default function DiffViewer(props: DiffViewerProps) {
  let containerRef: HTMLDivElement | undefined;
  const [collapsedFiles, setCollapsedFiles] = createSignal<Set<string>>(new Set());

  function toggleFile(path: string) {
    setCollapsedFiles((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  createEffect(() => {
    const path = props.selectedPath;
    if (path && containerRef) {
      const el = containerRef.querySelector(`[data-file-path="${CSS.escape(path)}"]`);
      if (el) {
        el.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    }
  });

  return (
    <div class="diff-viewer" ref={containerRef}>
      <Show
        when={props.diffs.length > 0}
        fallback={
          <div class="dv-empty">Select a file to view its diff</div>
        }
      >
        <For each={props.diffs}>
          {(diff) => (
            <div class="dv-file" data-file-path={diff.path}>
              <button
                class="dv-file-header"
                onClick={() => toggleFile(diff.path)}
              >
                <span class="dv-file-chevron">
                  {collapsedFiles().has(diff.path) ? "\u25B8" : "\u25BE"}
                </span>
                <span class="dv-file-status" data-status={diff.status}>
                  {statusLabel(diff.status)}
                </span>
                <span class="dv-file-path">{diff.path}</span>
                <span class="dv-file-stats">
                  <Show when={diff.additions > 0}>
                    <span class="dv-stat-add">+{diff.additions}</span>
                  </Show>
                  <Show when={diff.deletions > 0}>
                    <span class="dv-stat-del">-{diff.deletions}</span>
                  </Show>
                </span>
              </button>
              <Show when={!collapsedFiles().has(diff.path)}>
                <Show
                  when={!diff.binary}
                  fallback={
                    <div class="dv-binary">Binary file changed</div>
                  }
                >
                  <div class="dv-hunks">
                    <For each={diff.hunks}>
                      {(hunk) => <HunkView hunk={hunk} />}
                    </For>
                  </div>
                </Show>
              </Show>
            </div>
          )}
        </For>
      </Show>

      <style>{`
        .diff-viewer {
          font-family: var(--font-mono);
          font-size: 12px;
          line-height: 1.5;
          overflow-y: auto;
          flex: 1;
        }
        .dv-empty {
          display: flex;
          align-items: center;
          justify-content: center;
          height: 100%;
          color: var(--text-muted);
          font-family: var(--font-sans);
          font-size: 13px;
        }
        .dv-file {
          border-bottom: 1px solid var(--border);
        }
        .dv-file-header {
          display: flex;
          align-items: center;
          gap: 8px;
          width: 100%;
          padding: 6px 12px;
          text-align: left;
          background: var(--bg-tertiary);
          font-family: var(--font-mono);
          font-size: 12px;
          position: sticky;
          top: 0;
          z-index: 1;
          border-bottom: 1px solid var(--border);
        }
        .dv-file-header:hover {
          background: var(--bg-hover);
        }
        .dv-file-chevron {
          width: 10px;
          font-size: 10px;
          flex-shrink: 0;
        }
        .dv-file-status {
          font-size: 11px;
          padding: 1px 6px;
          border-radius: 3px;
          font-weight: 600;
          flex-shrink: 0;
        }
        .dv-file-status[data-status="added"] {
          color: var(--success);
          background: rgba(76, 175, 80, 0.15);
        }
        .dv-file-status[data-status="modified"] {
          color: var(--warning);
          background: rgba(255, 152, 0, 0.15);
        }
        .dv-file-status[data-status="deleted"] {
          color: var(--error);
          background: rgba(244, 67, 54, 0.15);
        }
        .dv-file-status[data-status="renamed"],
        .dv-file-status[data-status="copied"] {
          color: var(--accent);
          background: rgba(74, 158, 255, 0.15);
        }
        .dv-file-path {
          flex: 1;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
        .dv-file-stats {
          display: flex;
          gap: 6px;
          flex-shrink: 0;
        }
        .dv-stat-add { color: var(--success); }
        .dv-stat-del { color: var(--error); }
        .dv-binary {
          padding: 16px;
          color: var(--text-muted);
          text-align: center;
          font-style: italic;
        }
        .dv-hunk-header {
          padding: 4px 12px;
          color: var(--text-muted);
          background: rgba(74, 158, 255, 0.08);
          font-size: 11px;
          border-top: 1px solid var(--border);
          border-bottom: 1px solid var(--border);
        }
        .dv-line {
          display: flex;
          white-space: pre;
          min-height: 18px;
        }
        .dv-line-add {
          background: #2d4a2d;
        }
        .dv-line-del {
          background: #4a2d2d;
        }
        .dv-lineno {
          display: inline-block;
          width: 48px;
          padding: 0 6px;
          text-align: right;
          color: var(--text-muted);
          flex-shrink: 0;
          user-select: none;
        }
        .dv-lineno-old {
          border-right: 1px solid var(--border);
        }
        .dv-lineno-new {
          border-right: 1px solid var(--border);
        }
        .dv-prefix {
          display: inline-block;
          width: 16px;
          text-align: center;
          flex-shrink: 0;
          user-select: none;
        }
        .dv-line-add .dv-prefix { color: var(--success); }
        .dv-line-del .dv-prefix { color: var(--error); }
        .dv-content {
          flex: 1;
          padding-right: 12px;
        }
      `}</style>
    </div>
  );
}
