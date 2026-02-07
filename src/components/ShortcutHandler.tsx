import { createSignal, onMount, onCleanup } from "solid-js";
import { listShortcuts, onShortcutsUpdated } from "../lib/tauri";
import { buildShortcutMap, normalizeEvent } from "../lib/shortcut-parser";
import { dispatchAction } from "../lib/action-registry";
import type { UnlistenFn } from "@tauri-apps/api/event";

/**
 * Invisible component that attaches a single global keydown listener.
 * Loads shortcut config from backend on mount and listens for hot-reload updates.
 */
export default function ShortcutHandler() {
  const [shortcutMap, setShortcutMap] = createSignal<Map<string, string>>(
    new Map(),
  );

  let unlistenUpdate: UnlistenFn | null = null;

  onMount(async () => {
    // Load initial shortcuts from backend
    try {
      const config = await listShortcuts();
      const { map, conflicts } = buildShortcutMap(config.shortcuts);
      if (conflicts.length > 0) {
        console.warn("Shortcut conflicts detected:", conflicts);
      }
      setShortcutMap(map);
    } catch {
      // Backend may not have shortcuts configured yet — use empty map
      console.warn("Failed to load shortcuts config, using defaults");
      loadDefaults();
    }

    // Listen for hot-reload updates
    try {
      unlistenUpdate = await onShortcutsUpdated((config) => {
        const { map, conflicts } = buildShortcutMap(config.shortcuts);
        if (conflicts.length > 0) {
          console.warn("Shortcut conflicts detected:", conflicts);
        }
        setShortcutMap(map);
      });
    } catch {
      // Event listener registration may fail if backend doesn't emit
    }

    window.addEventListener("keydown", handleKeyDown);
  });

  onCleanup(() => {
    window.removeEventListener("keydown", handleKeyDown);
    unlistenUpdate?.();
  });

  function loadDefaults() {
    const defaults: Record<string, string> = {
      "workspace.new": "Cmd+N",
      "workspace.close": "Cmd+W",
      "tab.1": "Cmd+1",
      "tab.2": "Cmd+2",
      "tab.3": "Cmd+3",
      "tab.4": "Cmd+4",
      "tab.5": "Cmd+5",
      "tab.6": "Cmd+6",
      "tab.7": "Cmd+7",
      "tab.8": "Cmd+8",
      "tab.9": "Cmd+9",
      "tab.next": "Cmd+Tab",
      "tab.prev": "Cmd+Shift+Tab",
      "palette.snippets": "Cmd+P",
      "message.send": "Cmd+Enter",
      "settings.open": "Cmd+,",
      "agent.restart": "Cmd+Shift+R",
    };
    const { map } = buildShortcutMap(defaults);
    setShortcutMap(map);
  }

  function handleKeyDown(e: KeyboardEvent) {
    // Ignore modifier-only key presses
    if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;

    // Ignore events when focused in contenteditable or certain inputs
    const target = e.target as HTMLElement;
    if (
      target.tagName === "INPUT" ||
      target.tagName === "TEXTAREA" ||
      target.isContentEditable
    ) {
      // Only intercept specific shortcuts even in input fields
      const binding = normalizeEvent(e);
      const action = shortcutMap().get(binding);
      if (action === "settings.open" || action === "workspace.new") {
        if (dispatchAction(action)) {
          e.preventDefault();
        }
      }
      return;
    }

    const binding = normalizeEvent(e);
    const action = shortcutMap().get(binding);
    if (action && dispatchAction(action)) {
      e.preventDefault();
    }
  }

  return null;
}
