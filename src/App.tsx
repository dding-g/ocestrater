import { createSignal, createEffect, onCleanup, onMount, Show } from "solid-js";
import Sidebar from "./components/Sidebar";
import TabBar from "./components/TabBar";
import AgentPanel from "./components/AgentPanel";
import ReviewPanel from "./components/ReviewPanel";
import SnippetPalette from "./components/SnippetPalette";
import SnippetManager from "./components/SnippetManager";
import TrustDialog from "./components/TrustDialog";
import ShortcutHandler from "./components/ShortcutHandler";
import SettingsModal from "./components/SettingsModal";
import { state, activeWorkspace } from "./store/workspace-store";
import { onTrustRequired } from "./lib/tauri";
import { registerAction, unregisterAction } from "./lib/action-registry";
import type { TrustRequiredPayload } from "./lib/types";

export interface Workspace {
  id: string;
  repoPath: string;
  repoAlias: string;
  branch: string;
  agent: string;
  status: "idle" | "running" | "stopped";
}

export interface Repo {
  path: string;
  alias: string;
  workspaces: Workspace[];
}

export default function App() {
  const [sidebarWidth, setSidebarWidth] = createSignal(260);
  const [reviewWidth, setReviewWidth] = createSignal(400);
  const [showPalette, setShowPalette] = createSignal(false);
  const [showSnippetManager, setShowSnippetManager] = createSignal(false);
  const [showSettings, setShowSettings] = createSignal(false);
  const [trustPayload, setTrustPayload] = createSignal<TrustRequiredPayload | null>(null);

  // Register global actions via action registry (used by ShortcutHandler)
  onMount(() => {
    registerAction("palette.snippets", () => {
      if (activeWorkspace()) {
        setShowPalette((v) => !v);
      }
    });
    registerAction("settings.open", () => {
      setShowSettings((v) => !v);
    });
    registerAction("workspace.new", () => {
      // TODO: Open create workspace dialog
      // For now, this is a placeholder for future workspace creation flow
    });

    onCleanup(() => {
      unregisterAction("palette.snippets");
      unregisterAction("settings.open");
      unregisterAction("workspace.new");
    });
  });

  // Listen for trust-required events globally
  createEffect(() => {
    let unlisten: (() => void) | undefined;
    onTrustRequired((payload) => {
      setTrustPayload(payload);
    }).then((fn) => {
      unlisten = fn;
    });
    onCleanup(() => unlisten?.());
  });

  return (
    <div class="app-container">
      <Sidebar
        width={sidebarWidth()}
        onResize={setSidebarWidth}
      />
      <div class="main-panel">
        <TabBar />
        <AgentPanel />
      </div>
      <Show when={activeWorkspace()}>
        <ReviewPanel
          workspace={activeWorkspace()!}
          width={reviewWidth()}
          onResize={setReviewWidth}
        />
      </Show>

      {/* Snippet Palette overlay */}
      <Show when={showPalette() && activeWorkspace()}>
        <SnippetPalette
          workspaceId={activeWorkspace()!.id}
          repoPath={activeWorkspace()!.repoPath}
          onClose={() => setShowPalette(false)}
        />
      </Show>

      {/* Snippet Manager overlay */}
      <Show when={showSnippetManager()}>
        <SnippetManager
          repoPath={activeWorkspace()?.repoPath ?? null}
          onClose={() => setShowSnippetManager(false)}
        />
      </Show>

      {/* Trust Dialog */}
      <Show when={trustPayload()}>
        {(payload) => (
          <TrustDialog
            repoPath={payload().repo_path}
            scriptContent={payload().script_content}
            workspaceId={payload().workspace_id}
            changedFiles={
              payload().changed_files.length > 0
                ? payload().changed_files
                : undefined
            }
            onApprove={() => setTrustPayload(null)}
            onDeny={() => setTrustPayload(null)}
          />
        )}
      </Show>

      {/* Settings Modal */}
      <SettingsModal
        open={showSettings()}
        onClose={() => setShowSettings(false)}
      />

      {/* Global shortcut handler — must be always mounted */}
      <ShortcutHandler />
    </div>
  );
}
