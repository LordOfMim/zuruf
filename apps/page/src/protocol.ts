import type {
  Binding,
  ControlState,
  DomCapture,
  Rect,
} from "../../../packages/protocol/src/index";

export type Point = { x: number; y: number };

/** What the page agent tells the app. Sent through the `__zurufSend` binding. */
export type Outgoing =
  | { type: "ready"; payload: { top: boolean; visible: boolean } }
  | { type: "visibility"; payload: { visible: boolean } }
  | { type: "title"; payload: { title: string } }
  | { type: "pointer"; payload: { cursor: Point } }
  | { type: "action"; payload: { action: string } }
  /** A selection was started or ended in this frame; the app tells the tab's other frames. */
  | { type: "mode"; payload: { mode: "none" | "element" | "region" } }
  | {
      type: "capture.request";
      payload: {
        kind: string;
        cursor: Point;
        rect: Rect | null;
        dom: DomCapture | null;
      };
    };

/** What the app tells the page agent. Delivered by calling `__zurufReceive`. */
export type Incoming =
  | { type: "configure"; payload: { review: boolean; bindings: Binding[] } }
  | { type: "status"; payload: { recording: boolean } }
  | { type: "mode"; payload: { mode: "none" | "element" | "region" } }
  /** Where an agent moved or clicked, in the tab's viewport. */
  | { type: "agent.pointer"; payload: { x: number; y: number; pressed: boolean } }
  /** What an agent says it is doing. */
  | { type: "agent.note"; payload: { text: string } }
  /** An element an agent wants the person to look at. */
  | { type: "agent.point"; payload: { selector: string; text: string } }
  | { type: "control.changed"; payload: ControlState }
  | { type: "overlay.hide" | "overlay.show" | "capture.saved" | "dom.request" }
  | { type: "capture.failed"; payload: { reason: string } };

/** The value `__zurufReceive` returns for a given message. */
export type Reply =
  | { ok: true }
  | { hidden: true }
  | { found: boolean }
  | { dom: DomCapture | null; cursor: Point };

declare global {
  /** Exposed by the app in the agent's isolated world only; pages cannot reach it. */
  var __zurufSend: ((json: string) => void) | undefined;
  var __zurufReceive: ((message: Incoming) => Reply) | undefined;
}
