import type { Preset } from "../api/types";
import { templateFields } from "./template";

/** One-line description of a preset for lists. */
export function summarize(s: Preset): string {
	const parts: string[] = [];
	if (s.design) {
		const fields = templateFields(s.design);
		parts.push(`design ${s.design.width_mm}×${s.design.height_mm} mm`);
		if (fields.length) parts.push(`fields: ${fields.join(", ")}`);
	}
	if (s.paper && !s.design) parts.push(s.paper === "default" ? "default paper" : s.paper);
	if (s.orientation) parts.push(s.orientation);
	if (s.scale) parts.push(`scale ${s.scale}`);
	if (s.copies && s.copies > 1) parts.push(`${s.copies}×`);
	if (s.duplex && s.duplex !== "off") parts.push("duplex");
	if (s.color === "grayscale") parts.push("greyscale");
	if (s.pages) parts.push(`pages ${s.pages}`);
	return parts.join(" · ") || "all using defaults";
}
