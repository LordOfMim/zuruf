import type { ActionId, SessionMode, TaskStatus, WorkspaceStrategy } from "@zuruf/protocol";

/** All interface copy in one place (German). Short, concrete, no explanations where the layout already explains. */

export const actionLabel: Record<ActionId, string> = {
  "review.toggle": "Review ein/aus",
  "capture.viewport": "Ansicht aufnehmen",
  "capture.region": "Ausschnitt aufnehmen",
  "capture.dom": "Element aufnehmen",
  "comment.text": "Kommentar schreiben",
  "speech.toggle": "Diktat ein/aus",
  "batch.dispatch": "An neuen Agenten senden",
  "batch.send_to_thread": "An laufende Aufgabe senden",
  "control.takeover": "Steuerung übernehmen",
  "action.cancel": "Abbrechen",
  "panel.toggle": "Seitenleiste ein/aus",
  "app.open": "Zuruf öffnen",
  "profile.slot1": "Agentenprofil 1",
  "profile.slot2": "Agentenprofil 2",
  "profile.slot3": "Agentenprofil 3",
};

export const taskStatusLabel: Record<TaskStatus, string> = {
  finalizing_transcript: "Transkript wird fertig",
  queued: "Wartet",
  dispatching: "Wird gesendet",
  running: "Arbeitet",
  completed: "Fertig",
  needs_review: "Rückfrage",
  needs_approval: "Wartet auf Freigabe",
  failed: "Fehlgeschlagen",
  cancelled: "Abgebrochen",
  unknown_remote_state: "Zustand unklar",
};

export const strategyLabel: Record<WorkspaceStrategy, string> = {
  new_worktree: "neuer Worktree",
  existing_worktree: "bestehender Worktree",
  shared_root: "Projektordner",
};

export const sessionModeLabel: Record<SessionMode, string> = {
  shared: "Gemeinsam",
  isolated: "Getrennt",
  observer: "Beobachten",
};

/** Copy for daemon notices and request errors, keyed by their machine code. */
const noticeText: Record<string, string> = {
  "speech.gap": "Kurze Lücke in der Aufnahme.",
  "speech.unavailable": "Voxtype ist nicht erreichbar.",
  "speech.busy_external": "Voxtype nimmt gerade woanders auf.",
  "capture.unscriptable_page": "Diese Seite lässt sich nicht aufnehmen.",
  "capture.failed": "Aufnahme fehlgeschlagen.",
  "dispatch.failed": "Senden fehlgeschlagen. Nichts ging verloren.",
  "dispatch.workspace_conflict": "Zwei Agenten schreiben in denselben Ordner.",
  "t3.disconnected": "T3 Code ist nicht verbunden.",
  batch_empty: "Noch nichts aufgenommen.",
  batch_not_draft: "Dieser Stapel ist bereits gesendet.",
  bindings_invalid: "Diese Belegung ist nicht möglich.",
  request_failed: "Das hat nicht geklappt.",
  http_error: "Das hat nicht geklappt.",
};

export function noticeMessage(code: string, detail?: string | null): string {
  return noticeText[code] ?? detail ?? noticeText.request_failed!;
}

const relative = new Intl.RelativeTimeFormat("de", { numeric: "auto", style: "short" });

export function timeAgo(timestamp: string | null | undefined): string {
  if (!timestamp) return "";
  const seconds = Math.round((new Date(timestamp).getTime() - Date.now()) / 1000);
  if (Math.abs(seconds) < 45) return "gerade eben";
  const minutes = Math.round(seconds / 60);
  if (Math.abs(minutes) < 60) return relative.format(minutes, "minute");
  const hours = Math.round(minutes / 60);
  if (Math.abs(hours) < 24) return relative.format(hours, "hour");
  return relative.format(Math.round(hours / 24), "day");
}

/** "/angebote?filter=offen" from a full URL; falls back to the input. */
export function shortUrl(url: string | null | undefined): string {
  if (!url) return "";
  try {
    const parsed = new URL(url);
    return parsed.pathname + parsed.search;
  } catch {
    return url;
  }
}
