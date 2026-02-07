import { describe, it, expect, vi } from "vitest";
import {
  normalizeBinding,
  normalizeEvent,
  buildShortcutMap,
} from "../shortcut-parser";

describe("normalizeBinding", () => {
  it("normalizes Meta to Cmd", () => {
    expect(normalizeBinding("Meta+K")).toBe("Cmd+k");
  });

  it("normalizes Command to Cmd", () => {
    expect(normalizeBinding("Command+K")).toBe("Cmd+k");
  });

  it("normalizes Control to Ctrl", () => {
    expect(normalizeBinding("Control+C")).toBe("Ctrl+c");
  });

  it("normalizes Option to Alt", () => {
    expect(normalizeBinding("Option+X")).toBe("Alt+x");
  });

  it("normalizes Cmd (already canonical) unchanged", () => {
    expect(normalizeBinding("Cmd+P")).toBe("Cmd+p");
  });

  it("normalizes Ctrl (already canonical) unchanged", () => {
    expect(normalizeBinding("Ctrl+Z")).toBe("Ctrl+z");
  });

  it("produces canonical modifier order: Ctrl+Alt+Shift+Cmd+key", () => {
    // Reversed input order
    expect(normalizeBinding("Cmd+Shift+Alt+Ctrl+X")).toBe(
      "Ctrl+Alt+Shift+Cmd+x",
    );
  });

  it("produces canonical order regardless of input order", () => {
    expect(normalizeBinding("Shift+Cmd+K")).toBe("Shift+Cmd+k");
    expect(normalizeBinding("Cmd+Shift+K")).toBe("Shift+Cmd+k");
  });

  it("handles single key without modifiers", () => {
    expect(normalizeBinding("Escape")).toBe("escape");
  });

  it("lowercases the key part", () => {
    expect(normalizeBinding("Cmd+Enter")).toBe("Cmd+enter");
  });

  it("handles Ctrl+Alt combination", () => {
    expect(normalizeBinding("Alt+Ctrl+Delete")).toBe("Ctrl+Alt+delete");
  });

  it("handles spaces around plus signs", () => {
    expect(normalizeBinding("Cmd + Shift + R")).toBe("Shift+Cmd+r");
  });

  it("normalizes mixed alias modifiers", () => {
    expect(normalizeBinding("Command+Option+Shift+Z")).toBe(
      "Alt+Shift+Cmd+z",
    );
  });

  it("handles comma key", () => {
    expect(normalizeBinding("Cmd+,")).toBe("Cmd+,");
  });

  it("handles Tab key", () => {
    expect(normalizeBinding("Cmd+Tab")).toBe("Cmd+tab");
  });

  it("handles number keys", () => {
    expect(normalizeBinding("Cmd+1")).toBe("Cmd+1");
    expect(normalizeBinding("Cmd+9")).toBe("Cmd+9");
  });

  it("handles binding with no modifiers (just a key like F1)", () => {
    expect(normalizeBinding("F1")).toBe("f1");
    expect(normalizeBinding("Enter")).toBe("enter");
    expect(normalizeBinding("Backspace")).toBe("backspace");
  });

  it("handles empty binding string gracefully", () => {
    // An empty string split on "+" yields [""], so the key becomes ""
    const result = normalizeBinding("");
    expect(result).toBe("");
  });

  it("handles binding with duplicate modifiers", () => {
    // Duplicate modifiers should be collapsed by the Set
    const result = normalizeBinding("Cmd+Cmd+K");
    expect(result).toBe("Cmd+k");
  });

  it("handles binding with all same modifiers", () => {
    const result = normalizeBinding("Shift+Shift+Shift+A");
    expect(result).toBe("Shift+a");
  });

  it("handles binding with mixed case modifiers that alias to same canonical", () => {
    // "Meta" and "Command" both map to "Cmd"
    const result = normalizeBinding("Meta+Command+K");
    // Both are "Cmd", the key is "k" (last part)
    // But wait: "Meta+Command+K" splits to ["Meta", "Command", "K"]
    // modifiers = ["Meta", "Command"] -> both become "Cmd" in the Set -> Set has one "Cmd"
    // key = "k"
    expect(result).toBe("Cmd+k");
  });
});

describe("normalizeEvent", () => {
  function makeEvent(overrides: Partial<KeyboardEvent>): KeyboardEvent {
    return {
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      metaKey: false,
      key: "a",
      ...overrides,
    } as KeyboardEvent;
  }

  it("produces Cmd+key for metaKey", () => {
    const result = normalizeEvent(makeEvent({ metaKey: true, key: "p" }));
    expect(result).toBe("Cmd+p");
  });

  it("produces Ctrl+key for ctrlKey", () => {
    const result = normalizeEvent(makeEvent({ ctrlKey: true, key: "c" }));
    expect(result).toBe("Ctrl+c");
  });

  it("produces correct order with all modifiers", () => {
    const result = normalizeEvent(
      makeEvent({
        ctrlKey: true,
        altKey: true,
        shiftKey: true,
        metaKey: true,
        key: "z",
      }),
    );
    expect(result).toBe("Ctrl+Alt+Shift+Cmd+z");
  });

  it("normalizes space key", () => {
    const result = normalizeEvent(makeEvent({ metaKey: true, key: " " }));
    expect(result).toBe("Cmd+space");
  });

  it("preserves comma key", () => {
    const result = normalizeEvent(makeEvent({ metaKey: true, key: "," }));
    expect(result).toBe("Cmd+,");
  });

  it("lowercases letter keys", () => {
    const result = normalizeEvent(makeEvent({ key: "A" }));
    expect(result).toBe("a");
  });

  it("handles shift+letter", () => {
    const result = normalizeEvent(
      makeEvent({ shiftKey: true, key: "R" }),
    );
    expect(result).toBe("Shift+r");
  });

  it("produces just the key when no modifiers are pressed", () => {
    const result = normalizeEvent(makeEvent({ key: "Escape" }));
    expect(result).toBe("escape");
  });

  it("produces just the key for letter with no modifiers", () => {
    const result = normalizeEvent(makeEvent({ key: "a" }));
    expect(result).toBe("a");
  });

  it("handles function keys", () => {
    const result = normalizeEvent(makeEvent({ key: "F1" }));
    expect(result).toBe("f1");
  });

  it("handles Tab key", () => {
    const result = normalizeEvent(makeEvent({ metaKey: true, key: "Tab" }));
    expect(result).toBe("Cmd+tab");
  });

  it("handles Alt+Shift combination", () => {
    const result = normalizeEvent(
      makeEvent({ altKey: true, shiftKey: true, key: "p" }),
    );
    expect(result).toBe("Alt+Shift+p");
  });

  it("handles Enter key with modifier", () => {
    const result = normalizeEvent(
      makeEvent({ metaKey: true, key: "Enter" }),
    );
    expect(result).toBe("Cmd+enter");
  });
});

describe("buildShortcutMap", () => {
  it("builds reverse map from action->binding to binding->action", () => {
    const { map, conflicts } = buildShortcutMap({
      "workspace.new": "Cmd+N",
      "settings.open": "Cmd+,",
    });
    expect(map.get("Cmd+n")).toBe("workspace.new");
    expect(map.get("Cmd+,")).toBe("settings.open");
    expect(conflicts).toHaveLength(0);
  });

  it("detects conflicts when two actions share a binding", () => {
    const { map, conflicts } = buildShortcutMap({
      "action.a": "Cmd+K",
      "action.b": "Cmd+K",
    });
    expect(conflicts).toHaveLength(1);
    expect(conflicts[0].binding).toBe("Cmd+k");
    expect(conflicts[0].actions).toContain("action.a");
    expect(conflicts[0].actions).toContain("action.b");
    // First action wins
    expect(map.get("Cmd+k")).toBe("action.a");
  });

  it("RESERVED_SHORTCUTS correctly rejects OS-reserved keybindings", () => {
    // RESERVED_SHORTCUTS uses lowercase keys ("Cmd+q") which matches
    // normalizeBinding output. Reserved shortcuts are correctly rejected.
    const { map } = buildShortcutMap({
      "quit.action": "Cmd+Q",
    });
    // normalizeBinding("Cmd+Q") -> "Cmd+q" which matches "Cmd+q" in the set,
    // so the shortcut IS skipped as intended.
    expect(map.has("Cmd+q")).toBe(false);
  });

  it("reserved shortcuts are skipped for all reserved keys", () => {
    // Cmd+H and Cmd+M are OS-reserved and should be rejected.
    const { map } = buildShortcutMap({
      "hide.action": "Cmd+H",
      "minimize.action": "Cmd+M",
    });
    expect(map.has("Cmd+h")).toBe(false);
    expect(map.has("Cmd+m")).toBe(false);
  });

  it("non-reserved shortcuts are always included", () => {
    const { map, conflicts } = buildShortcutMap({
      "palette.open": "Cmd+P",
    });
    expect(map.get("Cmd+p")).toBe("palette.open");
    expect(conflicts).toHaveLength(0);
  });

  it("normalizes bindings before checking for conflicts", () => {
    const { conflicts } = buildShortcutMap({
      "action.a": "Command+K",
      "action.b": "Meta+K",
    });
    // Both normalize to Cmd+k, should conflict
    expect(conflicts).toHaveLength(1);
  });

  it("handles empty shortcuts config", () => {
    const { map, conflicts } = buildShortcutMap({});
    expect(map.size).toBe(0);
    expect(conflicts).toHaveLength(0);
  });

  it("handles single shortcut without conflicts", () => {
    const { map, conflicts } = buildShortcutMap({
      "test.action": "Ctrl+Shift+T",
    });
    expect(map.size).toBe(1);
    expect(map.get("Ctrl+Shift+t")).toBe("test.action");
    expect(conflicts).toHaveLength(0);
  });

  it("handles many shortcuts without conflicts (performance sanity)", () => {
    const shortcuts: Record<string, string> = {};
    for (let i = 0; i < 100; i++) {
      shortcuts[`action.${i}`] = `Ctrl+Shift+Alt+${String.fromCharCode(65 + (i % 26))}${i >= 26 ? i.toString() : ""}`;
    }
    // This should complete without performance issues
    const { map } = buildShortcutMap(shortcuts);
    expect(map.size).toBeGreaterThan(0);
  });

  it("detects multiple independent conflicts", () => {
    const { conflicts } = buildShortcutMap({
      "a.1": "Cmd+K",
      "a.2": "Cmd+K",
      "b.1": "Cmd+L",
      "b.2": "Cmd+L",
    });
    expect(conflicts).toHaveLength(2);
  });

  it("three-way conflict lists all three actions", () => {
    const { conflicts } = buildShortcutMap({
      "x.1": "Cmd+J",
      "x.2": "Meta+J",
      "x.3": "Command+J",
    });
    // All three normalize to Cmd+j
    expect(conflicts).toHaveLength(1);
    expect(conflicts[0].actions).toHaveLength(3);
  });

  it("preserves first-action-wins for three-way conflict", () => {
    const { map } = buildShortcutMap({
      "first.action": "Cmd+J",
      "second.action": "Meta+J",
      "third.action": "Command+J",
    });
    expect(map.get("Cmd+j")).toBe("first.action");
  });
});
