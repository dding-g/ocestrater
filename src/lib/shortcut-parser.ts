/**
 * Parses a shortcut config map into a normalized Map<string, string>
 * where keys are canonical keybinding strings and values are action names.
 *
 * Canonical modifier order: Ctrl+Alt+Shift+Cmd+<Key>
 */

const MODIFIER_ORDER = ["Ctrl", "Alt", "Shift", "Cmd"] as const;

const RESERVED_SHORTCUTS = new Set(["Cmd+Q", "Cmd+H", "Cmd+M"]);

export interface ShortcutConflict {
  binding: string;
  actions: string[];
}

/**
 * Normalize a keybinding string to canonical form: Ctrl+Alt+Shift+Cmd+Key
 */
export function normalizeBinding(binding: string): string {
  const parts = binding.split("+").map((p) => p.trim());
  const key = parts[parts.length - 1].toLowerCase();
  const modifiers = new Set(
    parts.slice(0, -1).map((m) => {
      const lower = m.toLowerCase();
      if (lower === "cmd" || lower === "meta" || lower === "command") return "Cmd";
      if (lower === "ctrl" || lower === "control") return "Ctrl";
      if (lower === "shift") return "Shift";
      if (lower === "alt" || lower === "option") return "Alt";
      return m;
    }),
  );

  const ordered = MODIFIER_ORDER.filter((m) => modifiers.has(m));
  return [...ordered, key].join("+");
}

/**
 * Convert a KeyboardEvent into a canonical keybinding string.
 */
export function normalizeEvent(e: KeyboardEvent): string {
  const modifiers: string[] = [];
  if (e.ctrlKey) modifiers.push("Ctrl");
  if (e.altKey) modifiers.push("Alt");
  if (e.shiftKey) modifiers.push("Shift");
  if (e.metaKey) modifiers.push("Cmd");

  // Normalize key name
  let key = e.key;
  // Map special keys
  if (key === ",") key = ",";
  else if (key === " ") key = "space";
  else key = key.toLowerCase();

  return [...modifiers, key].join("+");
}

/**
 * Build a lookup map from shortcut config (action -> binding) to (binding -> action).
 * Returns the map and any detected conflicts.
 */
export function buildShortcutMap(shortcuts: Record<string, string>): {
  map: Map<string, string>;
  conflicts: ShortcutConflict[];
} {
  const reverseMap = new Map<string, string[]>();
  const map = new Map<string, string>();
  const conflicts: ShortcutConflict[] = [];

  for (const [action, binding] of Object.entries(shortcuts)) {
    const normalized = normalizeBinding(binding);

    if (RESERVED_SHORTCUTS.has(normalized)) {
      console.warn(`Shortcut rejected: ${binding} (${action}) is a reserved OS shortcut`);
      continue;
    }

    const existing = reverseMap.get(normalized) || [];
    existing.push(action);
    reverseMap.set(normalized, existing);

    // First action wins
    if (!map.has(normalized)) {
      map.set(normalized, action);
    }
  }

  // Detect conflicts
  for (const [binding, actions] of reverseMap) {
    if (actions.length > 1) {
      console.warn(
        `Shortcut conflict: ${binding} is bound to both ${actions.join(" and ")}`,
      );
      conflicts.push({ binding, actions });
    }
  }

  return { map, conflicts };
}
