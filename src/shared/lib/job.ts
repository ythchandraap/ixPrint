import type { JobStatus } from "../api/types";

/** States after which a job's status no longer changes. */
export const FINAL_STATES = ["completed", "gone", "failed"];

export function describeJob(s: Pick<JobStatus, "state" | "position" | "ahead" | "message">): string {
	switch (s.state) {
		case "queued":
			return s.ahead ? `Queued #${s.position} (${s.ahead} job${s.ahead === 1 ? "" : "s"} ahead)` : "Queued, next to print";
		case "printing":
			return "Printing";
		case "paused":
			return "Held / paused";
		case "error":
			return `Error: ${s.message ?? "printer issue"}`;
		case "completed":
			return "Printed successfully";
		case "gone":
			return "No longer in the queue";
		case "failed":
			return `Failed: ${s.message ?? ""}`;
		default:
			return "Unknown status";
	}
}
