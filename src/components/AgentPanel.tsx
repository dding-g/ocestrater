import { createEffect, createSignal, on, onCleanup, onMount, Show } from "solid-js";
import {
  state,
  activeWorkspace,
  setWorkspaceModel,
} from "../store/workspace-store";
import {
  createTerminal,
  attachTerminal,
  detachTerminal,
  hasTerminal,
} from "../lib/terminal-cache";
import { sendToAgent, switchAgentModel, getConfig } from "../lib/tauri";
import { registerAction, unregisterAction } from "../lib/action-registry";
import ModelSelector from "./ModelSelector";

export default function AgentPanel() {
  let termContainer!: HTMLDivElement;
  const [input, setInput] = createSignal("");
  const [switching, setSwitching] = createSignal(false);
  const [agentModels, setAgentModels] = createSignal<Record<string, string[]>>({});
  let prevId: string | null = null;
  let inputRef: HTMLTextAreaElement | undefined;

  // Load agent model config
  onMount(async () => {
    try {
      const config = await getConfig();
      const agents = (config.agents ?? {}) as Record<string, { models?: string[] }>;
      const modelMap: Record<string, string[]> = {};
      for (const [name, agentConf] of Object.entries(agents)) {
        if (agentConf.models && agentConf.models.length > 0) {
          modelMap[name] = agentConf.models;
        }
      }
      setAgentModels(modelMap);
    } catch {
      // Config not available
    }

    // Register message.send action
    registerAction("message.send", () => {
      inputRef?.focus();
      sendMessage();
    });
  });

  onCleanup(() => {
    unregisterAction("message.send");
  });

  // React to activeId changes: detach old terminal, attach new one
  createEffect(
    on(
      () => state.activeId,
      (id) => {
        // Detach previous
        if (prevId) {
          detachTerminal(prevId);
        }

        // Attach new
        if (id && termContainer) {
          if (!hasTerminal(id)) {
            createTerminal(id);
          }
          attachTerminal(id, termContainer);
        }

        prevId = id;
      },
    ),
  );

  // ResizeObserver for terminal fit
  let observer: ResizeObserver | null = null;
  createEffect(() => {
    if (termContainer && !observer) {
      observer = new ResizeObserver(() => {
        const id = state.activeId;
        if (id) {
          // Re-attach triggers fit internally, but also handle resize
          attachTerminal(id, termContainer);
        }
      });
      observer.observe(termContainer);
      onCleanup(() => {
        observer?.disconnect();
        observer = null;
      });
    }
  });

  async function sendMessage() {
    const msg = input().trim();
    const ws = activeWorkspace();
    if (!msg || !ws) return;
    setInput("");

    try {
      await sendToAgent(ws.id, msg);
    } catch {
      // Dev mode fallback — write directly to terminal for feedback
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  async function handleModelSwitch(model: string) {
    const workspace = activeWorkspace();
    if (!workspace) return;
    setSwitching(true);
    try {
      setWorkspaceModel(workspace.id, model);
      await switchAgentModel(workspace.id, model);
    } catch {
      // Model switch may fail if backend doesn't support it yet
    } finally {
      setSwitching(false);
    }
  }

  const ws = () => activeWorkspace();
  const currentModels = () => {
    const workspace = ws();
    if (!workspace) return [];
    return agentModels()[workspace.agent] || [];
  };

  return (
    <div class="agent-panel">
      <Show
        when={ws()}
        fallback={
          <div class="agent-empty">
            <p>Select a workspace or create one to start</p>
          </div>
        }
      >
        <div class="agent-header">
          <span class="agent-ws-name">
            {ws()!.repoAlias} / {ws()!.branch}
          </span>
          <span class="agent-badge">{ws()!.agent}</span>
          <ModelSelector
            models={currentModels()}
            current={ws()!.model}
            onSelect={handleModelSwitch}
            disabled={switching()}
          />
          <span
            class="agent-status-badge"
            classList={{
              running: ws()!.status === "running",
              stopped: ws()!.status === "stopped",
            }}
          >
            {ws()!.status}
          </span>
        </div>
      </Show>

      <div class="agent-terminal" ref={termContainer} />

      <div class="agent-input-bar">
        <textarea
          ref={inputRef}
          class="agent-input"
          placeholder={
            ws()
              ? `Message ${ws()!.agent}...`
              : "Select a workspace..."
          }
          value={input()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          disabled={!ws()}
          rows={1}
        />
        <button
          class="agent-send"
          onClick={sendMessage}
          disabled={!ws() || !input().trim()}
        >
          Send
        </button>
      </div>

      <style>{`
        .agent-panel {
          flex: 1;
          display: flex;
          flex-direction: column;
          min-width: var(--panel-min);
          background: var(--bg-primary);
        }
        .agent-header {
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 8px 16px;
          height: var(--header-height);
          border-bottom: 1px solid var(--border);
          background: var(--bg-secondary);
        }
        .agent-ws-name {
          font-weight: 500;
          font-size: 13px;
          font-family: var(--font-mono);
        }
        .agent-badge {
          font-size: 11px;
          padding: 2px 8px;
          background: var(--bg-tertiary);
          border-radius: 10px;
          color: var(--text-secondary);
        }
        .agent-status-badge {
          font-size: 11px;
          padding: 2px 8px;
          border-radius: 10px;
          background: var(--bg-tertiary);
          color: var(--text-muted);
        }
        .agent-status-badge.running {
          background: rgba(76, 175, 80, 0.15);
          color: var(--success);
        }
        .agent-status-badge.stopped {
          background: rgba(244, 67, 54, 0.15);
          color: var(--error);
        }
        .agent-terminal {
          flex: 1;
          padding: 8px;
          overflow: hidden;
        }
        .agent-empty {
          flex: 1;
          display: flex;
          align-items: center;
          justify-content: center;
          color: var(--text-muted);
        }
        .agent-input-bar {
          display: flex;
          gap: 8px;
          padding: 10px 16px;
          border-top: 1px solid var(--border);
          background: var(--bg-secondary);
        }
        .agent-input {
          flex: 1;
          resize: none;
          min-height: 32px;
          max-height: 120px;
          line-height: 1.4;
        }
        .agent-send {
          padding: 6px 16px;
          background: var(--accent);
          color: white;
          border-radius: var(--radius);
          font-weight: 500;
          font-size: 12px;
          align-self: flex-end;
        }
        .agent-send:hover:not(:disabled) { background: var(--accent-hover); }
        .agent-send:disabled { opacity: 0.4; cursor: default; }
      `}</style>
    </div>
  );
}
