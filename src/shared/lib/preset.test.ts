import { describe, expect, it } from "vitest";
import type { Preset } from "../api/types";
import { summarize } from "./preset";

describe("summarize", () => {
	it("describes an empty preset", () => {
		expect(summarize({})).toBe("all using defaults");
	});

	it("summarizes a filled preset and its design fields", () => {
		const preset: Preset = {
			orientation: "landscape", scale: "fit", copies: 2, duplex: "long-edge", color: "grayscale", pages: "1-2",
			design: {
				width_mm: 80, height_mm: 50, sample: {},
				elements: [{ type: "text", id: "label", x: 0, y: 0, w: 20, h: 8, text: "{{name}}", size: 10, bold: false, align: "left", valign: "top", wrap: true, fit: true }],
			},
		};
		expect(summarize(preset)).toBe("design 80×50 mm · fields: name · landscape · scale fit · 2× · duplex · greyscale · pages 1-2");
	});
});
