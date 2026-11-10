import type { ActionId, Binding, Chord, Modifier } from "@zuruf/protocol";

const MODIFIER_ORDER: Modifier[] = ["ctrl", "super", "alt", "shift"];
const MODIFIER_LABEL: Record<Modifier, string> = { ctrl: "Strg", alt: "Alt", shift: "⇧", super: "Super" };

const CODE_LABEL: Record<string, string> = {
  Escape: "Esc",
  Enter: "↵",
  Space: "Leertaste",
  Backspace: "⌫",
  Tab: "Tab",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Period: ".",
  Comma: ",",
  Slash: "/",
  Semicolon: ";",
  Quote: "'",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backquote: "`",
  Backslash: "\\",
};

/** Human label for one key, layout independent: "KeyW" → "W", "Digit1" → "1". */
export function codeLabel(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  if (code.startsWith("Numpad")) return `Num ${code.slice(6)}`;
  return CODE_LABEL[code] ?? code;
}

/** Keycap labels in display order, e.g. ["⇧", "N"]. */
export function chordKeys(chord: Chord): string[] {
  const mods = MODIFIER_ORDER.filter((mod) => chord.mods.includes(mod)).map((mod) => MODIFIER_LABEL[mod]);
  return [...mods, codeLabel(chord.code)];
}

const MODIFIER_CODES = new Set([
  "ControlLeft",
  "ControlRight",
  "AltLeft",
  "AltRight",
  "ShiftLeft",
  "ShiftRight",
  "MetaLeft",
  "MetaRight",
  "OSLeft",
  "OSRight",
]);

/** Builds a chord from a keydown; null while only modifiers are held. */
export function chordFromEvent(event: KeyboardEvent): Chord | null {
  if (MODIFIER_CODES.has(event.code) || !event.code) return null;
  const mods: Modifier[] = [];
  if (event.ctrlKey) mods.push("ctrl");
  if (event.altKey) mods.push("alt");
  if (event.shiftKey) mods.push("shift");
  if (event.metaKey) mods.push("super");
  return { mods, code: event.code };
}

export function sameChord(a: Chord | null, b: Chord | null): boolean {
  if (!a || !b) return false;
  return a.code === b.code && a.mods.length === b.mods.length && a.mods.every((mod) => b.mods.includes(mod));
}

const page = (action: ActionId, code: string, mods: Modifier[] = []): Binding => ({
  action,
  scope: "page",
  chord: { mods, code },
});
const global = (action: ActionId, code: string, mods: Modifier[]): Binding => ({
  action,
  scope: "global",
  chord: { mods, code },
});

/** Mirrors the daemon defaults (docs/architecture.md); used by the demo transport and "reset". */
export function defaultBindings(): Binding[] {
  return [
    page("capture.viewport", "KeyW"),
    page("capture.region", "KeyS"),
    page("capture.dom", "KeyD"),
    page("comment.text", "KeyE"),
    page("speech.toggle", "KeyR"),
    page("batch.dispatch", "KeyN", ["shift"]),
    page("action.cancel", "Escape"),
    page("panel.toggle", "KeyP"),
    { action: "batch.send_to_thread", scope: "page", chord: null },
    page("profile.slot1", "Digit1", ["shift"]),
    page("profile.slot2", "Digit2", ["shift"]),
    page("profile.slot3", "Digit3", ["shift"]),
    global("review.toggle", "KeyR", ["super", "alt"]),
    global("control.takeover", "Period", ["super", "alt"]),
    global("app.open", "KeyZ", ["super", "alt"]),
    { action: "batch.dispatch", scope: "global", chord: null },
    { action: "speech.toggle", scope: "global", chord: null },
  ];
}
