import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock all xterm dependencies before importing the module under test.
// We use classes instead of arrow functions so they can be used with `new`.
vi.mock("@xterm/xterm", () => {
  class MockTerminal {
    dispose = vi.fn();
    write = vi.fn();
    writeln = vi.fn();
    onData = vi.fn();
    loadAddon = vi.fn();
    open = vi.fn();
  }
  return { Terminal: MockTerminal };
});

vi.mock("@xterm/addon-fit", () => {
  class MockFitAddon {
    fit = vi.fn();
    dispose = vi.fn();
  }
  return { FitAddon: MockFitAddon };
});

vi.mock("@xterm/addon-webgl", () => {
  class MockWebglAddon {
    dispose = vi.fn();
  }
  return { WebglAddon: MockWebglAddon };
});

// Mock tauri IPC functions
vi.mock("../tauri", () => ({
  onPtyOutput: vi.fn().mockImplementation(() => Promise.resolve(vi.fn())),
  onPtyExit: vi.fn().mockImplementation(() => Promise.resolve(vi.fn())),
}));

// Mock workspace-store functions
vi.mock("../../store/workspace-store", () => ({
  markNewOutput: vi.fn(),
  updateWorkspaceStatus: vi.fn(),
  incrementOutputBytes: vi.fn(),
}));

// Mock DOM APIs needed by terminal-cache
vi.stubGlobal("document", {
  createElement: vi.fn(() => ({
    style: { width: "", height: "" },
    parentElement: null,
    appendChild: vi.fn(),
    removeChild: vi.fn(),
  })),
});

vi.stubGlobal("requestAnimationFrame", vi.fn((cb: () => void) => cb()));

import {
  createTerminal,
  hasTerminal,
  destroyTerminal,
  writeToTerminal,
  attachTerminal,
  detachTerminal,
} from "../terminal-cache";

describe("terminal-cache", () => {
  beforeEach(() => {
    // Destroy any cached terminals from prior tests
    for (const id of ["ws-1", "ws-2", "ws-3", "ws-a", "ws-b", "ws-c"]) {
      if (hasTerminal(id)) {
        destroyTerminal(id);
      }
    }
  });

  describe("createTerminal", () => {
    it("creates a terminal entry with Terminal instance", () => {
      const entry = createTerminal("ws-1");
      expect(entry).toBeDefined();
      expect(entry.terminal).toBeDefined();
      expect(entry.fitAddon).toBeDefined();
      expect(entry.element).toBeDefined();
      expect(entry.unlisten).toBeDefined();
    });

    it("returns existing entry if already cached", () => {
      const entry1 = createTerminal("ws-1");
      const entry2 = createTerminal("ws-1");
      expect(entry1).toBe(entry2);
    });

    it("creates distinct entries for different workspace IDs", () => {
      const entry1 = createTerminal("ws-a");
      const entry2 = createTerminal("ws-b");
      expect(entry1).not.toBe(entry2);
    });

    it("calls terminal.loadAddon with the fit addon", () => {
      const entry = createTerminal("ws-1");
      expect(entry.terminal.loadAddon).toHaveBeenCalledWith(entry.fitAddon);
    });

    it("calls terminal.open with the element", () => {
      const entry = createTerminal("ws-1");
      expect(entry.terminal.open).toHaveBeenCalledWith(entry.element);
    });
  });

  describe("hasTerminal", () => {
    it("returns false for non-existent id", () => {
      expect(hasTerminal("nonexistent")).toBe(false);
    });

    it("returns true after creating a terminal", () => {
      createTerminal("ws-1");
      expect(hasTerminal("ws-1")).toBe(true);
    });

    it("returns false after destroying a terminal", () => {
      createTerminal("ws-1");
      destroyTerminal("ws-1");
      expect(hasTerminal("ws-1")).toBe(false);
    });
  });

  describe("destroyTerminal", () => {
    it("removes the terminal from cache", () => {
      createTerminal("ws-1");
      expect(hasTerminal("ws-1")).toBe(true);
      destroyTerminal("ws-1");
      expect(hasTerminal("ws-1")).toBe(false);
    });

    it("calls terminal.dispose()", () => {
      const entry = createTerminal("ws-1");
      const disposeSpy = entry.terminal.dispose;
      destroyTerminal("ws-1");
      expect(disposeSpy).toHaveBeenCalledTimes(1);
    });

    it("does not throw for non-existent id", () => {
      expect(() => destroyTerminal("nonexistent")).not.toThrow();
    });

    it("only removes the specified terminal, leaving others intact", () => {
      createTerminal("ws-a");
      createTerminal("ws-b");
      destroyTerminal("ws-a");
      expect(hasTerminal("ws-a")).toBe(false);
      expect(hasTerminal("ws-b")).toBe(true);
    });
  });

  describe("writeToTerminal", () => {
    it("calls terminal.write with the provided data", () => {
      const entry = createTerminal("ws-1");
      writeToTerminal("ws-1", "hello world");
      expect(entry.terminal.write).toHaveBeenCalledWith("hello world");
    });

    it("does not throw for non-existent id", () => {
      expect(() => writeToTerminal("nonexistent", "data")).not.toThrow();
    });

    it("can write multiple times to the same terminal", () => {
      const entry = createTerminal("ws-1");
      writeToTerminal("ws-1", "line1");
      writeToTerminal("ws-1", "line2");
      expect(entry.terminal.write).toHaveBeenCalledTimes(2);
      expect(entry.terminal.write).toHaveBeenCalledWith("line1");
      expect(entry.terminal.write).toHaveBeenCalledWith("line2");
    });
  });

  describe("attachTerminal", () => {
    it("does not throw for non-existent id", () => {
      const parent = { appendChild: vi.fn() } as any;
      expect(() => attachTerminal("nonexistent", parent)).not.toThrow();
    });

    it("appends the terminal element to the parent", () => {
      const entry = createTerminal("ws-1");
      const parent = { appendChild: vi.fn() } as any;
      attachTerminal("ws-1", parent);
      expect(parent.appendChild).toHaveBeenCalledWith(entry.element);
    });
  });

  describe("detachTerminal", () => {
    it("does not throw for non-existent id", () => {
      expect(() => detachTerminal("nonexistent")).not.toThrow();
    });
  });

  describe("multiple entries coexistence", () => {
    it("can create and access multiple terminal entries", () => {
      createTerminal("ws-a");
      createTerminal("ws-b");
      createTerminal("ws-c");

      expect(hasTerminal("ws-a")).toBe(true);
      expect(hasTerminal("ws-b")).toBe(true);
      expect(hasTerminal("ws-c")).toBe(true);
    });

    it("destroying one does not affect others", () => {
      createTerminal("ws-a");
      createTerminal("ws-b");
      createTerminal("ws-c");

      destroyTerminal("ws-b");

      expect(hasTerminal("ws-a")).toBe(true);
      expect(hasTerminal("ws-b")).toBe(false);
      expect(hasTerminal("ws-c")).toBe(true);
    });

    it("each terminal entry has independent write functions", () => {
      const entryA = createTerminal("ws-a");
      const entryB = createTerminal("ws-b");

      writeToTerminal("ws-a", "data-a");

      expect(entryA.terminal.write).toHaveBeenCalledWith("data-a");
      expect(entryB.terminal.write).not.toHaveBeenCalled();
    });
  });
});
