import { describe, expect, it } from "vitest";
import type { Template, TemplateElement } from "../api/types";
import { templateAutoFields, templateFields } from "./template";

const makeTemplate = (elements: TemplateElement[]): Template => ({
	width_mm: 80,
	height_mm: 50,
	elements,
	sample: {},
});

const text = (value: string): TemplateElement => ({
	type: "text", id: "text", x: 0, y: 0, w: 20, h: 8, text: value,
	size: 10, bold: false, align: "left", valign: "top", wrap: true, fit: true,
});

describe("templateFields and templateAutoFields", () => {
	it("excludes automatic fields from input fields", () => {
		const design = makeTemplate([text("{{name}} / {{ no }} / {{halaman}} / {{NAME}}")]);
		expect(templateFields(design)).toEqual(["NAME", "name"]);
		expect(templateAutoFields(design)).toEqual(["no", "halaman"]);
	});

	it("recognizes placeholders on barcode, QR, and data-backed images", () => {
		const design = makeTemplate([
			{ type: "barcode", id: "barcode", x: 0, y: 0, w: 20, h: 8, value: "{{sku}}", show_text: true },
			{ type: "qr", id: "qr", x: 0, y: 0, size: 12, value: "{{url}}" },
			{ type: "image", id: "image", x: 0, y: 0, w: 8, h: 8, src: "{{photo}}", fit: "contain", grayscale: false },
		]);
		expect(templateFields(design)).toEqual(["photo", "sku", "url"]);
	});
});
