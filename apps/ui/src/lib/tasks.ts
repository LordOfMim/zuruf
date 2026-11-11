import type { Task } from "@zuruf/protocol";

export type TaskGroup = "attention" | "active" | "finished" | "settled";

const ATTENTION = new Set<Task["status"]>(["needs_approval", "needs_review", "failed", "unknown_remote_state"]);
const FINISHED = new Set<Task["status"]>(["completed", "cancelled"]);
const SETTLEABLE = new Set<Task["status"]>(["completed", "cancelled", "failed", "needs_review", "unknown_remote_state"]);

/** Acknowledging a task is independent of whether its agent succeeded or was interrupted. */
export function taskGroup(task: Task): TaskGroup {
  if (task.markedDone) return "settled";
  if (ATTENTION.has(task.status)) return "attention";
  return FINISHED.has(task.status) ? "finished" : "active";
}

export function canSettleTask(task: Task): boolean {
  return task.markedDone || SETTLEABLE.has(task.status);
}
