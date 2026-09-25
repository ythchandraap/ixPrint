import { describe, expect, it } from "vitest";
import { describeJob } from "./job";

describe("describeJob", () => {
	it("describes queued jobs with and without jobs ahead", () => {
		expect(describeJob({ state: "queued", position: 3, ahead: 2 })).toBe("Queued #3 (2 jobs ahead)");
		expect(describeJob({ state: "queued", position: 1 })).toBe("Queued, next to print");
		expect(describeJob({ state: "queued", position: 2, ahead: 1 })).toBe("Queued #2 (1 job ahead)");
	});

	it.each([
		["printing", "Printing"],
		["paused", "Held / paused"],
		["error", "Error: printer issue"],
  ["completed", "Spooler reports completed"],
  ["gone", "Left the queue; print outcome unknown"],
	] as const)("describes %s", (state, expected) => {
		expect(describeJob({ state })).toBe(expected);
	});

	it("includes a supplied printer error", () => {
		expect(describeJob({ state: "error", message: "Paper out" })).toBe("Error: Paper out");
	});
});
