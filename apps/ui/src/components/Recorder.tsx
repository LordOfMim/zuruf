import { Show, createEffect, onCleanup, onMount } from "solid-js";
import type { AppStore } from "../lib/store";
import { Keys } from "./basics";

const BAR_WIDTH = 2;
const BAR_GAP = 2;

/**
 * The microphone line. A rolling level history is painted on a canvas so the
 * 15 Hz level stream never touches the DOM or the reactive graph.
 */
export function Recorder(props: { store: AppStore }) {
  const speech = () => props.store.status()?.speech;
  const recording = () => speech()?.state === "recording";
  let canvas!: HTMLCanvasElement;
  const history: number[] = [];
  let frame = 0;

  const paint = () => {
    const context = canvas.getContext("2d");
    if (!context) return;
    const ratio = window.devicePixelRatio || 1;
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (canvas.width !== width * ratio || canvas.height !== height * ratio) {
      canvas.width = width * ratio;
      canvas.height = height * ratio;
    }
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    const capacity = Math.floor(width / (BAR_WIDTH + BAR_GAP));
    while (history.length > capacity) history.shift();
    context.fillStyle = getComputedStyle(canvas).color;
    const offset = capacity - history.length;
    history.forEach((level, index) => {
      const bar = Math.max(2, Math.min(1, level * 1.6) * height);
      const x = (offset + index) * (BAR_WIDTH + BAR_GAP);
      // Older bars fade out towards the left edge.
      context.globalAlpha = 0.25 + 0.75 * ((offset + index) / capacity);
      context.beginPath();
      context.roundRect(x, (height - bar) / 2, BAR_WIDTH, bar, 1);
      context.fill();
    });
    context.globalAlpha = 1;
  };

  createEffect(() => {
    const level = props.store.state.level;
    if (!recording()) return;
    history.push(level);
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(paint);
  });

  createEffect(() => {
    if (!recording()) {
      history.length = 0;
      frame = requestAnimationFrame(paint);
    }
  });

  onMount(paint);
  onCleanup(() => cancelAnimationFrame(frame));

  const hint = () => {
    switch (speech()?.state) {
      case "unavailable":
        return speech()?.error ?? "Diktat nicht verfügbar";
      case "busy_external":
        return "Voxtype nimmt woanders auf";
      case "error":
        return speech()?.error ?? "Diktat gestört";
      case "idle":
      case "transcribing":
        return "Diktat aus";
      default:
        return "";
    }
  };

  return (
    <button
      type="button"
      class="recorder"
      classList={{ "is-live": recording(), "is-off": !recording() }}
      aria-pressed={recording()}
      aria-label={recording() ? "Diktat pausieren" : "Diktat starten"}
      disabled={speech()?.state === "unavailable"}
      onClick={() => props.store.commands.action("speech.toggle")}
    >
      <span class="recorder-dot" />
      <canvas ref={canvas} class="recorder-wave" />
      <Show when={!recording()}>
        <span class="recorder-hint">{hint()}</span>
      </Show>
      <Show when={(speech()?.pendingSegments ?? 0) > 0}>
        <span class="recorder-pending" title="Transkription läuft">
          <span class="spinner" />
        </span>
      </Show>
      <Keys chord={props.store.bindingFor("speech.toggle")} />
    </button>
  );
}
