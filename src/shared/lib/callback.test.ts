import { describe, expect, it } from "vitest";
import type { CallbackStatus } from "../api/types";
import { describeCallback } from "./callback";

const callback = (state: CallbackStatus["state"], extra: Partial<CallbackStatus> = {}): CallbackStatus => ({
	url: "https://example.com/hooks/print",
	state,
	attempts: 2,
	time: "2026-01-01T00:00:00Z",
	...extra,
});

describe("describeCallback", () => {
	it("describes pending callback states", () => {
		expect(describeCallback(callback("watching"))).toBe("Callback to example.com: waiting for the job to finish");
		expect(describeCallback(callback("retrying", { error: "503" }))).toBe("Callback to example.com: attempt 2 failed (503), retrying");
	});

	it.each(["completed", "sent", "failed", "gone", "timeout", "error"])("includes the job result %s", (result) => {
		expect(describeCallback(callback("delivered", { result }))).toBe(`Callback delivered to example.com · result ${result}`);
	});

	it("describes callback failure and refusal", () => {
		expect(describeCallback(callback("failed", { error: "timeout" }))).toBe("Callback to example.com failed after 2 attempts: timeout");
		expect(describeCallback(callback("refused", { error: "unsafe URL" }))).toBe("Callback not run: unsafe URL");
	});
});
