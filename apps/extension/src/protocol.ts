import type {
  Binding,
  ControlState,
  DomCapture,
  Rect,
} from "../../../packages/protocol/src/index";
export type Point = { x: number; y: number };
export interface Bootstrap {
  port: number;
  sessionId: string;
  token: string;
  protocolVersion: number;
}
export interface ExtensionTab {
  tabId: number;
  targetId: string;
  url: string;
  title: string;
  zoom: number;
  active: boolean;
  scriptable: boolean;
}
export type Message =
  | { type: "status"; payload: { recording: boolean } }
  | {
      type:
        | "hello"
        | "frame.ready"
        | "resume"
        | "heartbeat"
        | "overlay.hide"
        | "overlay.show"
        | "capture.saved"
        | "dom.request";
    }
  | { type: "ack" | "overlay.hidden"; payload: { requestId: string } }
  | { type: "configure"; payload: { review: boolean; bindings: Binding[] } }
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
    }
  | { type: "capture.failed"; payload: { reason: string } }
  | { type: "control.changed"; payload: ControlState }
  | { type: "tabs"; payload: { tabs: ExtensionTab[] } }
  | {
      type: "dom.result";
      payload: { requestId: string; dom: DomCapture | null; cursor: Point };
    };
export type Envelope = Message & {
  v: number;
  requestId: string;
  sessionId: string;
  tabId?: number;
  frameId?: number;
  documentId?: string;
  timestamp: number;
};
