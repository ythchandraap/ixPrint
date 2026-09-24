import { invoke } from "../client";
import type { JobResult, PrintSettings } from "../types";

export const printCommands = {
	testPrint: (printer: string | null, preset: string | null, pages = 1) =>
		invoke<JobResult>("test_print", { printer, preset, pages }),
	/** Submits raw JSON or a PDF path through the local POST /print endpoint. */
	/** Submits raw JSON or a PDF path through the local POST /print endpoint. */
	appPrint: (input: { jsonBody?: string; pdfPath?: string; preset?: string | null; settings?: PrintSettings }) =>
		invoke<JobResult>("app_print", {
			jsonBody: input.jsonBody ?? null,
			pdfPath: input.pdfPath ?? null,
			preset: input.preset ?? null,
			settings: input.settings ?? {},
		}),
	/** PNG data URL of the first sheet a PDF would print as, laid out exactly like the real job. */
	/** PNG data URL of the first sheet a PDF would print as, laid out exactly like the real job. */
	previewPdfSheet: (pdfPath: string, preset: string | null, settings: PrintSettings) =>
		invoke<string>("preview_pdf_sheet", { pdfPath, preset, settings }),
};
