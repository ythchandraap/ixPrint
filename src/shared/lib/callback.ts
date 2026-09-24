import type { CallbackStatus } from "../api/types";

/** Callbacks still in progress; the history refreshes until they settle. */
export const CALLBACK_PENDING = ["watching", "retrying"];

/** One line for a callback's delivery state. */
export function describeCallback(c: CallbackStatus): string {
  let host = c.url;
  try {
    host = new URL(c.url).host;
  } catch {
    // keep the raw URL
  }
  switch (c.state) {
    case "watching":
      return `Callback to ${host}: waiting for the job to finish`;
    case "retrying":
      return `Callback to ${host}: attempt ${c.attempts} failed (${c.error ?? "?"}), retrying`;
    case "delivered":
      return `Callback delivered to ${host}${c.result ? ` · result ${c.result}` : ""}`;
    case "failed":
      return `Callback to ${host} failed after ${c.attempts} attempts: ${c.error ?? "?"}`;
    case "refused":
      return `Callback not run: ${c.error ?? "?"}`;
  }
}
