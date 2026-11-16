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

export { Icon, type IconName } from "./icons";

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
