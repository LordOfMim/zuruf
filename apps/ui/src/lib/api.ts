import type { ApiError, ServerEnvelope } from "@zuruf/protocol";

/** Transport abstraction so the real daemon and the in-browser demo share every line of UI code. */
export interface Transport {
  request<T>(method: string, path: string, body?: unknown): Promise<T>;
  /** Calls `onEvent` for every server event; `onState` reports the connection. Returns an unsubscribe function. */
  subscribe(onEvent: (event: ServerEnvelope) => void, onState: (state: LinkState) => void): () => void;
  /** Resolves an asset path to an object URL (cached). */
  asset(path: string): Promise<string>;
}

export type LinkState = "connecting" | "online" | "offline";

export class RequestError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    readonly body?: ApiError,
  ) {
    super(message);
  }
}

export interface Bootstrap {
  /** Origin of the daemon, e.g. http://127.0.0.1:47600. Empty: same origin. */
  origin: string;
  token: string;
}

const TOKEN_KEY = "zuruf.token";

/**
 * Finds the daemon and a token:
 * - side panel inside the extension: bootstrap.json next to the panel page
 * - app window: one-time code in the URL fragment (#code=…), exchanged once
 * - afterwards: token kept in sessionStorage for reloads
 */
export async function loadBootstrap(): Promise<Bootstrap | null> {
  if (location.protocol === "chrome-extension:") {
    const response = await fetch("../bootstrap.json");
    const data = (await response.json()) as { port: number; token: string };
    return { origin: `http://127.0.0.1:${data.port}`, token: data.token };
  }

  const fragment = new URLSearchParams(location.hash.slice(1));
  const code = fragment.get("code");
  if (code) {
    history.replaceState(null, "", location.pathname + location.search);
    const response = await fetch("/api/v1/auth/exchange", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ code }),
    });
    if (response.ok) {
      const { token } = (await response.json()) as { token: string };
      sessionStorage.setItem(TOKEN_KEY, token);
      return { origin: "", token };
    }
  }

  const stored = sessionStorage.getItem(TOKEN_KEY);
  return stored ? { origin: "", token: stored } : null;
}

export function createHttpTransport(bootstrap: Bootstrap): Transport {
  const base = `${bootstrap.origin}/api/v1`;

  async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const response = await fetch(base + path, {
      method,
      headers: {
        authorization: `Bearer ${bootstrap.token}`,
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!response.ok) {
      let parsed: ApiError | undefined;
      try {
        parsed = (await response.json()) as ApiError;
      } catch {
        parsed = undefined;
      }
      throw new RequestError(
        response.status,
        parsed?.error.code ?? "http_error",
        parsed?.error.message ?? response.statusText,
        parsed,
      );
    }
    if (response.status === 204) return undefined as T;
    return (await response.json()) as T;
  }

  function subscribe(onEvent: (event: ServerEnvelope) => void, onState: (state: LinkState) => void) {
    let socket: WebSocket | null = null;
    let closed = false;
    let attempt = 0;
    let timer: number | undefined;

    const wsBase = (bootstrap.origin || location.origin).replace(/^http/, "ws");

    const connect = () => {
      onState(attempt === 0 ? "connecting" : "offline");
      socket = new WebSocket(`${wsBase}/api/v1/events`);
      socket.onopen = () => {
        socket?.send(JSON.stringify({ type: "auth", token: bootstrap.token }));
      };
      socket.onmessage = (message) => {
        const event = JSON.parse(message.data as string) as ServerEnvelope;
        if (event.type === "snapshot") {
          attempt = 0;
          onState("online");
        }
        onEvent(event);
      };
      socket.onclose = () => {
        if (closed) return;
        onState("offline");
        attempt += 1;
        // The daemon is local: retry fast, back off gently, never give up.
        timer = window.setTimeout(connect, Math.min(250 * 2 ** Math.min(attempt, 4), 4000));
      };
    };
    connect();

    return () => {
      closed = true;
      window.clearTimeout(timer);
      socket?.close();
    };
  }

  const assets = new Map<string, Promise<string>>();
  function asset(path: string): Promise<string> {
    let pending = assets.get(path);
    if (!pending) {
      // Images cannot send an Authorization header, so fetch them and hand out object URLs.
      pending = fetch(bootstrap.origin + path, { headers: { authorization: `Bearer ${bootstrap.token}` } })
        .then((response) => {
          if (!response.ok) throw new RequestError(response.status, "asset_unavailable", response.statusText);
          return response.blob();
        })
        .then((blob) => URL.createObjectURL(blob));
      pending.catch(() => assets.delete(path));
      assets.set(path, pending);
    }
    return pending;
  }

  return {
    request,
    subscribe,
    asset,
  };
}
