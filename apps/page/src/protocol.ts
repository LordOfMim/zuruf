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
  | { type: "control.changed"; payload: ControlState }
  | { type: "overlay.hide" | "overlay.show" | "capture.saved" | "dom.request" }
  | { type: "capture.failed"; payload: { reason: string } };

/** The value `__zurufReceive` returns for a given message. */
export type Reply =
  | { ok: true }
  | { hidden: true }
  | { dom: DomCapture | null; cursor: Point };

declare global {
  /** Exposed by the app in the agent's isolated world only; pages cannot reach it. */
  var __zurufSend: ((json: string) => void) | undefined;
  var __zurufReceive: ((message: Incoming) => Reply) | undefined;
}
