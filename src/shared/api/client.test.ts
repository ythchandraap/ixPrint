import { describe, expect, it } from "vitest";
import { errorText } from "./client";

describe("errorText", () => {
	it("handles strings, Error instances, and other values", () => {
		expect(errorText("offline")).toBe("offline");
		expect(errorText(new Error("failed"))).toBe("failed");
		expect(errorText({ message: "busy" })).toBe('{"message":"busy"}');
		expect(errorText(42)).toBe("42");
	});
});
