import { createSignal, createMemo, For, Show } from "solid-js";
import type { FileChange, FileStatus } from "../../lib/types";

interface FileTreeProps {
  files: FileChange[];
  selectedPath: string | null;
  onSelectFile: (path: string) => void;
}

interface DirNode {
  name: string;
  path: string;
  files: FileChange[];
  dirs: DirNode[];
}

function statusIcon(status: FileStatus): string {
  switch (status) {
    case "added": return "A";
    case "modified": return "M";
    case "deleted": return "D";
    case "renamed": return "R";
    case "copied": return "C";
  }
}

function statusColor(status: FileStatus): string {
  switch (status) {
    case "added": return "var(--success)";
    case "modified": return "var(--warning)";
    case "deleted": return "var(--error)";
    case "renamed": return "var(--accent)";
    case "copied": return "var(--accent)";
  }
}

function buildTree(files: FileChange[]): DirNode {
  const root: DirNode = { name: "", path: "", files: [], dirs: [] };

  for (const file of files) {
    const parts = file.path.split("/");
    let current = root;

    for (let i = 0; i < parts.length - 1; i++) {
      const dirName = parts[i];
      const dirPath = parts.slice(0, i + 1).join("/");
      let child = current.dirs.find((d) => d.name === dirName);
      if (!child) {
        child = { name: dirName, path: dirPath, files: [], dirs: [] };
        current.dirs.push(child);
      }
      current = child;
    }
    current.files.push(file);
  }

  return root;
}

function fileName(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1];
}

function DirGroup(props: {
  node: DirNode;
  depth: number;
  defaultExpanded: boolean;
  selectedPath: string | null;
  onSelectFile: (path: string) => void;
}) {
  const [expanded, setExpanded] = createSignal(props.defaultExpanded);

  return (
    <div class="ft-dir">
      <button
        class="ft-dir-header"
        style={{ "padding-left": `${8 + props.depth * 12}px` }}
        onClick={() => setExpanded(!expanded())}
      >
        <span class="ft-chevron">{expanded() ? "\u25BE" : "\u25B8"}</span>
        <span class="ft-dir-name">{props.node.name}</span>
        <span class="ft-dir-count">
          {countFiles(props.node)}
        </span>
      </button>
      <Show when={expanded()}>
        <For each={props.node.dirs}>
          {(dir) => (
            <DirGroup
              node={dir}
              depth={props.depth + 1}
              defaultExpanded={props.defaultExpanded}
              selectedPath={props.selectedPath}
              onSelectFile={props.onSelectFile}
            />
          )}
        </For>
        <For each={props.node.files}>
          {(file) => (
            <button
              class="ft-file"
              classList={{ "ft-file-active": props.selectedPath === file.path }}
              style={{ "padding-left": `${20 + (props.depth + 1) * 12}px` }}
              onClick={() => props.onSelectFile(file.path)}
            >
              <span class="ft-status" style={{ color: statusColor(file.status) }}>
                {statusIcon(file.status)}
              </span>
              <span class="ft-name">{fileName(file.path)}</span>
              <span class="ft-stats">
                <Show when={file.additions > 0}>
                  <span class="ft-add">+{file.additions}</span>
                </Show>
                <Show when={file.deletions > 0}>
                  <span class="ft-del">-{file.deletions}</span>
                </Show>
              </span>
            </button>
          )}
        </For>
      </Show>
    </div>
  );
}

function countFiles(node: DirNode): number {
  let count = node.files.length;
  for (const dir of node.dirs) {
    count += countFiles(dir);
  }
  return count;
}

export default function FileTree(props: FileTreeProps) {
  const tree = createMemo(() => buildTree(props.files));
  const defaultExpanded = createMemo(() => props.files.length <= 20);

  return (
    <div class="file-tree">
      <For each={tree().dirs}>
        {(dir) => (
          <DirGroup
            node={dir}
            depth={0}
            defaultExpanded={defaultExpanded()}
            selectedPath={props.selectedPath}
            onSelectFile={props.onSelectFile}
          />
        )}
      </For>
      <For each={tree().files}>
        {(file) => (
          <button
            class="ft-file"
            classList={{ "ft-file-active": props.selectedPath === file.path }}
            style={{ "padding-left": "20px" }}
            onClick={() => props.onSelectFile(file.path)}
          >
            <span class="ft-status" style={{ color: statusColor(file.status) }}>
              {statusIcon(file.status)}
            </span>
            <span class="ft-name">{fileName(file.path)}</span>
            <span class="ft-stats">
              <Show when={file.additions > 0}>
                <span class="ft-add">+{file.additions}</span>
              </Show>
              <Show when={file.deletions > 0}>
                <span class="ft-del">-{file.deletions}</span>
              </Show>
            </span>
          </button>
        )}
      </For>

      <style>{`
        .file-tree {
          padding: 4px 0;
          font-family: var(--font-mono);
          font-size: 12px;
        }
        .ft-dir-header {
          display: flex;
          align-items: center;
          gap: 4px;
          width: 100%;
          padding: 3px 8px;
          text-align: left;
          color: var(--text-secondary);
          font-size: 12px;
          font-family: var(--font-mono);
        }
        .ft-dir-header:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
        .ft-chevron {
          width: 10px;
          font-size: 10px;
          flex-shrink: 0;
        }
        .ft-dir-name {
          font-weight: 600;
        }
        .ft-dir-count {
          margin-left: auto;
          color: var(--text-muted);
          font-size: 11px;
        }
        .ft-file {
          display: flex;
          align-items: center;
          gap: 6px;
          width: 100%;
          padding: 3px 14px;
          text-align: left;
          font-family: var(--font-mono);
          font-size: 12px;
        }
        .ft-file:hover {
          background: var(--bg-hover);
        }
        .ft-file-active {
          background: var(--bg-tertiary);
        }
        .ft-status {
          font-weight: 600;
          width: 14px;
          text-align: center;
          flex-shrink: 0;
        }
        .ft-name {
          flex: 1;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }
        .ft-stats {
          display: flex;
          gap: 4px;
          font-size: 11px;
          flex-shrink: 0;
        }
        .ft-add { color: var(--success); }
        .ft-del { color: var(--error); }
      `}</style>
    </div>
  );
}
