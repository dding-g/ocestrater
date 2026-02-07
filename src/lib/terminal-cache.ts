import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { onPtyOutput, onPtyExit } from "./tauri";
import {
  markNewOutput,
  updateWorkspaceStatus,
  incrementOutputBytes,
} from "../store/workspace-store";

interface CachedTerminal {
  terminal: Terminal;
  fitAddon: FitAddon;
  element: HTMLDivElement;
  unlisten: UnlistenFn[];
}

const cache = new Map<string, CachedTerminal>();

export function createTerminal(workspaceId: string): CachedTerminal {
  if (cache.has(workspaceId)) {
    return cache.get(workspaceId)!;
  }

  const terminal = new Terminal({
    theme: {
      background: "#1a1a1a",
      foreground: "#e8e8e8",
      cursor: "#4a9eff",
      selectionBackground: "#4a9eff44",
    },
    fontFamily: "SF Mono, Menlo, Monaco, monospace",
    fontSize: 13,
    lineHeight: 1.4,
    cursorBlink: true,
    scrollback: 10000,
    convertEol: true,
  });

  const fitAddon = new FitAddon();
  terminal.loadAddon(fitAddon);

  // Create an offscreen container
  const element = document.createElement("div");
  element.style.width = "100%";
  element.style.height = "100%";

  // Open terminal into the offscreen container
  terminal.open(element);

  // Subscribe to IPC events
  const unlistenHandles: UnlistenFn[] = [];

  onPtyOutput(workspaceId, (data) => {
    terminal.write(data);
    markNewOutput(workspaceId);
    incrementOutputBytes(workspaceId, data.length);
  }).then((fn) => unlistenHandles.push(fn));

  onPtyExit(workspaceId, () => {
    updateWorkspaceStatus(workspaceId, "stopped");
    terminal.writeln("\r\n\x1b[90m[Process exited]\x1b[0m");
  }).then((fn) => unlistenHandles.push(fn));

  const entry: CachedTerminal = {
    terminal,
    fitAddon,
    element,
    unlisten: unlistenHandles,
  };

  cache.set(workspaceId, entry);
  return entry;
}

export function attachTerminal(
  workspaceId: string,
  parentElement: HTMLDivElement,
): void {
  const entry = cache.get(workspaceId);
  if (!entry) return;

  parentElement.appendChild(entry.element);

  // Schedule fit after DOM attachment
  requestAnimationFrame(() => {
    try {
      entry.fitAddon.fit();
    } catch {
      // Terminal may not be visible yet
    }
  });
}

export function detachTerminal(workspaceId: string): void {
  const entry = cache.get(workspaceId);
  if (!entry) return;

  if (entry.element.parentElement) {
    entry.element.parentElement.removeChild(entry.element);
  }
}

export function destroyTerminal(workspaceId: string): void {
  const entry = cache.get(workspaceId);
  if (!entry) return;

  // Unsubscribe IPC listeners
  for (const fn of entry.unlisten) {
    fn();
  }

  // Detach from DOM
  if (entry.element.parentElement) {
    entry.element.parentElement.removeChild(entry.element);
  }

  entry.terminal.dispose();
  cache.delete(workspaceId);
}

export function writeToTerminal(workspaceId: string, data: string): void {
  const entry = cache.get(workspaceId);
  if (!entry) return;
  entry.terminal.write(data);
}

export function hasTerminal(workspaceId: string): boolean {
  return cache.has(workspaceId);
}
