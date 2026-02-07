import { createSignal, Show } from "solid-js";
import type { WorkspaceTab } from "../../store/workspace-store";
import type { WorktreeStatus, MergeStrategy, MergeResult } from "../../lib/types";

interface ReviewActionsProps {
  workspace: WorkspaceTab;
  status: WorktreeStatus | null;
  onMerge: (strategy: MergeStrategy, message?: string) => void;
  onDiscard: () => void;
  merging: boolean;
  mergeResult: MergeResult | null;
}

export default function ReviewActions(props: ReviewActionsProps) {
  const [strategy, setStrategy] = createSignal<MergeStrategy>("squash");
  const [commitMessage, setCommitMessage] = createSignal("");
  const [showDiscard, setShowDiscard] = createSignal(false);

  const isRunning = () => props.workspace.status === "running";
  const disabled = () => isRunning() || props.merging || !props.status;

  function handleMerge() {
    const msg = strategy() === "squash" && commitMessage().trim()
      ? commitMessage().trim()
      : undefined;
    props.onMerge(strategy(), msg);
  }

  function handleDiscard() {
    setShowDiscard(false);
    props.onDiscard();
  }

  return (
    <div class="ra-container">
      <Show when={props.mergeResult}>
        {(result) => (
          <div
            class="ra-banner"
            classList={{
              "ra-banner-success": result().success,
              "ra-banner-error": !result().success,
            }}
          >
            <Show
              when={result().success}
              fallback={
                <span>
                  Merge conflicts detected in {result().conflicts.length} file(s)
                </span>
              }
            >
              <span>
                Changes merged successfully
                {result().merge_sha ? ` (${result().merge_sha!.slice(0, 7)})` : ""}
              </span>
            </Show>
          </div>
        )}
      </Show>

      <Show when={props.status?.has_conflicts}>
        <div class="ra-banner ra-banner-error">
          Conflicts detected in {props.status!.conflict_files.length} file(s)
        </div>
      </Show>

      <div class="ra-row">
        <div class="ra-strategy">
          <select
            class="ra-select"
            value={strategy()}
            onChange={(e) => setStrategy(e.currentTarget.value as MergeStrategy)}
            disabled={disabled()}
          >
            <option value="merge">Merge</option>
            <option value="squash">Squash</option>
            <option value="rebase">Rebase</option>
          </select>
        </div>

        <button
          class="ra-btn ra-btn-merge"
          onClick={handleMerge}
          disabled={disabled()}
        >
          {props.merging ? "Merging..." : "Merge"}
        </button>

        <button
          class="ra-btn ra-btn-discard"
          onClick={() => setShowDiscard(true)}
          disabled={props.merging}
        >
          Discard
        </button>
      </div>

      <Show when={strategy() === "squash"}>
        <input
          class="ra-commit-input"
          type="text"
          placeholder={`feat: ${props.workspace.branch}`}
          value={commitMessage()}
          onInput={(e) => setCommitMessage(e.currentTarget.value)}
          disabled={disabled()}
        />
      </Show>

      <Show when={showDiscard()}>
        <div class="ra-confirm-overlay">
          <div class="ra-confirm">
            <p class="ra-confirm-text">
              Discard all changes in <strong>{props.workspace.branch}</strong>?
              This deletes the worktree and branch. This cannot be undone.
            </p>
            <div class="ra-confirm-actions">
              <button
                class="ra-btn ra-btn-cancel"
                onClick={() => setShowDiscard(false)}
              >
                Cancel
              </button>
              <button
                class="ra-btn ra-btn-discard"
                onClick={handleDiscard}
              >
                Discard
              </button>
            </div>
          </div>
        </div>
      </Show>

      <style>{`
        .ra-container {
          border-top: 1px solid var(--border);
          padding: 8px 12px;
          display: flex;
          flex-direction: column;
          gap: 8px;
          flex-shrink: 0;
        }
        .ra-banner {
          padding: 6px 10px;
          border-radius: var(--radius);
          font-size: 12px;
          text-align: center;
        }
        .ra-banner-success {
          background: rgba(76, 175, 80, 0.15);
          color: var(--success);
        }
        .ra-banner-error {
          background: rgba(244, 67, 54, 0.15);
          color: var(--error);
        }
        .ra-row {
          display: flex;
          gap: 6px;
          align-items: center;
        }
        .ra-select {
          font-family: var(--font-sans);
          font-size: 12px;
          padding: 4px 8px;
          background: var(--bg-tertiary);
          color: var(--text-primary);
          border: 1px solid var(--border);
          border-radius: var(--radius);
          cursor: pointer;
          outline: none;
        }
        .ra-select:focus {
          border-color: var(--accent);
        }
        .ra-select:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
        .ra-btn {
          padding: 4px 12px;
          border-radius: var(--radius);
          font-size: 12px;
          font-weight: 500;
          cursor: pointer;
        }
        .ra-btn:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }
        .ra-btn-merge {
          background: var(--accent);
          color: #fff;
        }
        .ra-btn-merge:hover:not(:disabled) {
          background: var(--accent-hover);
        }
        .ra-btn-discard {
          background: rgba(244, 67, 54, 0.15);
          color: var(--error);
        }
        .ra-btn-discard:hover:not(:disabled) {
          background: rgba(244, 67, 54, 0.25);
        }
        .ra-btn-cancel {
          background: var(--bg-tertiary);
          color: var(--text-secondary);
        }
        .ra-btn-cancel:hover {
          background: var(--bg-hover);
        }
        .ra-commit-input {
          width: 100%;
          font-size: 12px;
          padding: 4px 8px;
        }
        .ra-confirm-overlay {
          position: fixed;
          top: 0;
          left: 0;
          right: 0;
          bottom: 0;
          background: rgba(0, 0, 0, 0.5);
          display: flex;
          align-items: center;
          justify-content: center;
          z-index: 100;
        }
        .ra-confirm {
          background: var(--bg-secondary);
          border: 1px solid var(--border);
          border-radius: var(--radius);
          padding: 16px;
          max-width: 360px;
        }
        .ra-confirm-text {
          font-size: 13px;
          margin-bottom: 12px;
          line-height: 1.5;
          color: var(--text-primary);
        }
        .ra-confirm-actions {
          display: flex;
          justify-content: flex-end;
          gap: 8px;
        }
      `}</style>
    </div>
  );
}
