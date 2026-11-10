import type { Chord, TaskStatus } from "@zuruf/protocol";
import { For, type JSX, Show, createResource, splitProps } from "solid-js";
import { chordKeys } from "../lib/chord";

/** Keycaps for a chord. Renders nothing for an unbound action. */
export function Keys(props: { chord: Chord | null | undefined; class?: string }) {
  return (
    <Show when={props.chord}>
      {(chord) => (
        <span class={`keys ${props.class ?? ""}`} aria-hidden="true">
          <For each={chordKeys(chord())}>{(key) => <kbd>{key}</kbd>}</For>
        </span>
      )}
    </Show>
  );
}

type ButtonProps = JSX.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "solid" | "quiet" | "ghost" | "danger";
  size?: "sm" | "md";
};

export function Button(props: ButtonProps) {
  const [own, rest] = splitProps(props, ["variant", "size", "class", "children"]);
  return (
    <button
      type="button"
      class={`btn btn-${own.variant ?? "quiet"} btn-${own.size ?? "md"} ${own.class ?? ""}`}
      {...rest}
    >
      {own.children}
    </button>
  );
}

const ICONS = {
  viewport: "M3 5.5A1.5 1.5 0 0 1 4.5 4h11A1.5 1.5 0 0 1 17 5.5v9a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5zM3 7.5h14",
  region: "M4 7V5a1 1 0 0 1 1-1h2M13 4h2a1 1 0 0 1 1 1v2M16 13v2a1 1 0 0 1-1 1h-2M7 16H5a1 1 0 0 1-1-1v-2",
  dom: "M7.5 6 4 10l3.5 4M12.5 6 16 10l-3.5 4",
  text: "M5 5h10M5 9h10M5 13h6",
  mic: "M10 3a2 2 0 0 0-2 2v4a2 2 0 1 0 4 0V5a2 2 0 0 0-2-2zM5.5 9.5a4.5 4.5 0 0 0 9 0M10 14v3",
  send: "M4 10h11M11 5.5l4.5 4.5-4.5 4.5",
  close: "M5.5 5.5l9 9M14.5 5.5l-9 9",
  check: "M4.5 10.5l3.5 3.5 7.5-8",
  chevron: "M6 8l4 4 4-4",
  external: "M8 5H5.5A1.5 1.5 0 0 0 4 6.5v8A1.5 1.5 0 0 0 5.5 16h8a1.5 1.5 0 0 0 1.5-1.5V12M11 4h5v5M16 4l-7 7",
  retry: "M15.5 10a5.5 5.5 0 1 1-1.7-4M15.5 4v3h-3",
  trash: "M5 6.5h10M8 6.5V5h4v1.5M6.5 6.5l.6 9h5.8l.6-9",
  tasks: "M4 5.5h1.5M8 5.5h8M4 10h1.5M8 10h8M4 14.5h1.5M8 14.5h8",
  browser: "M3 5.5A1.5 1.5 0 0 1 4.5 4h11A1.5 1.5 0 0 1 17 5.5v9a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5zM3 7.5h14M5.3 5.8h.01M7.3 5.8h.01",
  settings: "M4 6h7M14 6h2M4 14h2M9 14h7M11 4.5v3M6 12.5v3",
  review: "M10 4c-3.5 0-6 3.2-7 6 1 2.8 3.5 6 7 6s6-3.2 7-6c-1-2.8-3.5-6-7-6zM10 12.2a2.2 2.2 0 1 0 0-4.4 2.2 2.2 0 0 0 0 4.4z",
  hand: "M7 10V5.5a1.2 1.2 0 0 1 2.4 0V9M9.4 9V4.7a1.2 1.2 0 0 1 2.4 0V9M11.8 9V5.7a1.2 1.2 0 0 1 2.4 0v5.8c0 2.8-1.9 4.5-4.5 4.5-2 0-3.2-.9-4.2-2.6L4 10.9a1.1 1.1 0 0 1 1.9-1.1L7 11.4",
  plus: "M10 4.5v11M4.5 10h11",
  warn: "M10 4 3.5 15.5h13zM10 8.5v3.2M10 13.6h.01",
} as const;

export type IconName = keyof typeof ICONS;

export function Icon(props: { name: IconName; size?: number; class?: string }) {
  return (
    <svg
      class={`icon ${props.class ?? ""}`}
      width={props.size ?? 16}
      height={props.size ?? 16}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d={ICONS[props.name]} />
    </svg>
  );
}

const STATUS_TONE: Record<TaskStatus, string> = {
  finalizing_transcript: "pending",
  queued: "pending",
  dispatching: "busy",
  running: "busy",
  completed: "ok",
  needs_review: "attention",
  needs_approval: "attention",
  failed: "danger",
  cancelled: "muted",
  unknown_remote_state: "danger",
};

export const taskTone = (status: TaskStatus) => STATUS_TONE[status];

/** One glyph per task state: the shape carries the meaning, colour only supports it. */
export function StatusGlyph(props: { status: TaskStatus }) {
  return <span class={`glyph glyph-${STATUS_TONE[props.status]}`} aria-hidden="true" />;
}

/** Screenshot thumbnail that resolves authenticated asset URLs lazily. */
export function Shot(props: { url: string | undefined; resolve: (path: string) => Promise<string>; alt: string }) {
  const [source] = createResource(
    () => props.url,
    (url) => (url.startsWith("data:") || url.startsWith("blob:") ? url : props.resolve(url)),
  );
  return (
    <span class="shot">
      <Show when={source()}>{(src) => <img src={src()} alt={props.alt} loading="lazy" decoding="async" />}</Show>
    </span>
  );
}
