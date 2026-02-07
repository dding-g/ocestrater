import { Show, For } from "solid-js";
import { grantTrust } from "../lib/tauri";
import { invoke } from "@tauri-apps/api/core";

interface TrustDialogProps {
  repoPath: string;
  scriptContent: string;
  workspaceId: string;
  changedFiles?: string[];
  onApprove: () => void;
  onDeny: () => void;
}

export default function TrustDialog(props: TrustDialogProps) {
  const isChanged = () => props.changedFiles && props.changedFiles.length > 0;

  async function handleApprove() {
    try {
      await grantTrust(props.repoPath);
      await invoke("run_setup_and_start_agent", {
        workspaceId: props.workspaceId,
      });
    } catch {
      // Proceed even if setup fails
    }
    props.onApprove();
  }

  async function handleDeny() {
    try {
      await invoke("start_agent_no_setup", {
        workspaceId: props.workspaceId,
      });
    } catch {
      // Proceed
    }
    props.onDeny();
  }

  return (
    <div class="td-overlay">
      <div class="td-modal">
        <div class="td-header">
          <span class="td-warning-icon">&#9888;</span>
          <span class="td-header-text">
            {isChanged()
              ? "Repository scripts have changed"
              : "Repository wants to execute scripts"}
          </span>
        </div>

        <div class="td-body">
          <div class="td-repo-path">
            <span class="td-label">Repository:</span>
            <code>{props.repoPath}</code>
          </div>

          <Show when={isChanged()}>
            <div class="td-changed">
              <span class="td-label">Changed files:</span>
              <ul class="td-changed-list">
                <For each={props.changedFiles}>
                  {(file) => <li>{file}</li>}
                </For>
              </ul>
            </div>
          </Show>

          <div class="td-script-section">
            <span class="td-label">Setup script:</span>
            <pre class="td-script-code"><code>{props.scriptContent}</code></pre>
          </div>
        </div>

        <div class="td-actions">
          <button class="td-btn td-btn-primary" onClick={handleApprove}>
            Trust & Run
          </button>
          <button class="td-btn td-btn-secondary" onClick={handleDeny}>
            Skip
          </button>
        </div>
      </div>

      <style>{`
        .td-overlay {
          position: fixed;
          inset: 0;
          z-index: 3000;
          background: rgba(0, 0, 0, 0.6);
          backdrop-filter: blur(4px);
          display: flex;
          align-items: center;
          justify-content: center;
        }
        .td-modal {
          width: 560px;
          max-height: 80vh;
          background: var(--bg-secondary);
          border: 1px solid var(--border);
          border-radius: 12px;
          box-shadow: 0 16px 48px rgba(0, 0, 0, 0.5);
          display: flex;
          flex-direction: column;
          overflow: hidden;
        }
        .td-header {
          display: flex;
          align-items: center;
          gap: 10px;
          padding: 16px 20px;
          border-bottom: 1px solid var(--border);
        }
        .td-warning-icon {
          font-size: 20px;
          color: var(--warning);
        }
        .td-header-text {
          font-size: 15px;
          font-weight: 600;
          color: var(--text-primary);
        }
        .td-body {
          flex: 1;
          overflow-y: auto;
          padding: 16px 20px;
          display: flex;
          flex-direction: column;
          gap: 12px;
        }
        .td-label {
          font-size: 11px;
          font-weight: 500;
          color: var(--text-secondary);
          text-transform: uppercase;
          letter-spacing: 0.5px;
          display: block;
          margin-bottom: 4px;
        }
        .td-repo-path code {
          font-family: var(--font-mono);
          font-size: 12px;
          color: var(--text-primary);
          background: var(--bg-tertiary);
          padding: 4px 8px;
          border-radius: 4px;
          display: inline-block;
        }
        .td-changed-list {
          list-style: none;
          padding: 0;
          display: flex;
          flex-direction: column;
          gap: 2px;
        }
        .td-changed-list li {
          font-family: var(--font-mono);
          font-size: 12px;
          color: var(--warning);
          padding: 2px 0;
        }
        .td-script-section {
          flex: 1;
        }
        .td-script-code {
          margin: 0;
          padding: 12px;
          background: var(--bg-primary);
          border: 1px solid var(--border);
          border-radius: var(--radius);
          font-family: var(--font-mono);
          font-size: 12px;
          line-height: 1.5;
          color: var(--text-primary);
          overflow-x: auto;
          max-height: 240px;
          overflow-y: auto;
          white-space: pre-wrap;
          word-break: break-all;
        }
        .td-actions {
          display: flex;
          gap: 8px;
          padding: 16px 20px;
          border-top: 1px solid var(--border);
          justify-content: flex-end;
        }
        .td-btn {
          padding: 8px 20px;
          font-size: 13px;
          border-radius: var(--radius);
          font-weight: 500;
        }
        .td-btn-primary {
          background: var(--accent);
          color: #fff;
        }
        .td-btn-primary:hover {
          background: var(--accent-hover);
        }
        .td-btn-secondary {
          background: var(--bg-tertiary);
          color: var(--text-secondary);
        }
        .td-btn-secondary:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }
      `}</style>
    </div>
  );
}
