// Typed wrappers around the Rust commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

/** A preset or request override; unset fields fall through to the next layer. */
export interface PrintSettings {
	printer?: string;
	copies?: number;
	collate?: boolean;
	orientation?: "portrait" | "landscape" | "auto";
	/** default | A4 | Letter | ... | "80mm" (roll) | "100x150mm" (custom) */
	paper?: string;
	/** shrink | fit | none | "90%" */
	scale?: string;
	align?: "center" | "top" | "top-left";
	/** mm: "5" | "5,3" | "5,3,5,3" */
	margins?: string;
	offset_x?: number;
	offset_y?: number;
	/** "1-3,5,8-" */
	pages?: string;
	duplex?: "off" | "long-edge" | "short-edge";
	color?: "color" | "grayscale";
	tray?: string;
	dpi?: number;
}

export interface ApiToken {
	id: string;
	name: string;
	/** First characters of the token, e.g. "fxp_1a2b3c…". */
	hint: string;
	created_at: string;
}

export interface TokenUse {
	time: string;
	client?: string;
}

/** Placeholder values for one printed page. */
export type TemplateRecord = Record<string, string>;

interface Box {
	x: number;
	y: number;
	w: number;
	h: number;
}

/** Template elements; positions and sizes in mm from the top-left corner. */
export type TemplateElement =
	| ({
			type: "text";
			id: string;
			/** May contain {{field}} placeholders and line breaks. */
			text: string;
			/** pt */
			size: number;
			bold: boolean;
			align: "left" | "center" | "right";
			valign: "top" | "middle" | "bottom";
			wrap: boolean;
			/** Shrink the font until the text fits. */
			fit: boolean;
	  } & Box)
	| { type: "line"; id: string; x1: number; y1: number; x2: number; y2: number; /** pt */ width: number }
	| ({ type: "rect"; id: string; /** pt, 0 = none */ stroke: number; fill: boolean } & Box)
	| ({ type: "barcode"; id: string; value: string; show_text: boolean } & Box)
	| { type: "qr"; id: string; x: number; y: number; size: number; value: string }
	| ({
			type: "image";
			id: string;
			/** Data URL, or "{{field}}" whose data value is one (e.g. a photo per record). */
			src: string;
			fit: "contain" | "stretch";
			grayscale: boolean;
	  } & Box);

/**
 * label: one page per record. list: records as rows under a header, the header
 * repeated on every page; the editor shows the bands header / one row / footer stacked.
 */
export type DesignLayout =
	| { mode: "label" }
	| {
			mode: "list";
			header_mm: number;
			row_mm: number;
			footer_mm: number;
			/** last_page: once after the last row; every_page: bottom of each page. */
			footer: "last_page" | "every_page";
			/** Page height follows the content (receipt roll). */
			roll: boolean;
	  };

/** Fields ix.print fills in itself. */
export const AUTO_FIELDS = ["no", "halaman", "total_halaman", "jumlah_data"];

/** A preset's WYSIWYG design (label/receipt with {{field}} placeholders). */
export interface Template {
	/** Page width. */
	width_mm: number;
	/** Page height (unused for a roll list). */
	height_mm: number;
	layout?: DesignLayout;
	elements: TemplateElement[];
	/** Example data for the preview and test prints. */
	sample: TemplateRecord;
}

/** Automatic fields a design uses ({{no}}, {{halaman}}...); clients may send their own values. */
export function templateAutoFields(t: Template): string[] {
	const names = new Set<string>();
	for (const el of t.elements) {
		const src =
			el.type === "text" ? el.text : el.type === "barcode" || el.type === "qr" ? el.value : el.type === "image" ? el.src : "";
		for (const m of src.matchAll(/\{\{\s*([A-Za-z0-9_.-]+)\s*\}\}/g)) {
			if (AUTO_FIELDS.includes(m[1])) names.add(m[1]);
		}
	}
	return AUTO_FIELDS.filter((f) => names.has(f));
}

/** Placeholder names used by a template, sorted (same rule as the Rust side). */
export function templateFields(t: Template): string[] {
	const names = new Set<string>();
	for (const el of t.elements) {
		const src =
			el.type === "text"
				? el.text
				: el.type === "barcode" || el.type === "qr"
					? el.value
					: el.type === "image" && el.src.includes("{{")
						? el.src
						: "";
		for (const m of src.matchAll(/\{\{\s*([A-Za-z0-9_.-]+)\s*\}\}/g)) {
			if (!AUTO_FIELDS.includes(m[1])) names.add(m[1]);
		}
	}
	return [...names].sort();
}

/** Print settings plus an optional design; with a design, clients send only data (JSON). */
export type Preset = PrintSettings & { design?: Template };

export interface Config {
	version: number;
	server_port: number;
	require_token: boolean;
	bridge_enabled: boolean;
	/** Websites allowed to call the API from a browser; "*" = any. */
	allowed_origins: string[];
	tokens: ApiToken[];
	default_printer?: string;
	default_preset?: string;
	presets: Record<string, Preset>;
	history_retention_days: number;
}

export interface PrinterView {
	name: string;
	status: string;
	/** Problem reported by the printer: paper out, offline, ... */
	message?: string;
	shared: string;
	os_default: boolean;
	/** The printer ix.print uses when nothing else is specified. */
	is_default: boolean;
	/** Name of the other ix.print this printer belongs to. */
	remote?: string;
}

export interface RemoteView {
	id: string;
	name: string;
	url: string;
	preset: string | null;
	token_hint: string;
	created_at: string;
	online: boolean | null;
	last_error: string | null;
	printers: number;
}

export interface PaperInfo {
	name: string;
	width_mm: number;
	height_mm: number;
}

export interface Capabilities {
	papers: PaperInfo[];
	default_paper?: PaperInfo;
	trays: string[];
	default_tray?: string;
	duplex: boolean;
	color: boolean;
	max_width_mm?: number;
	max_height_mm?: number;
}

export type NetProtocol = "ipp" | "raw" | "lpd";

export type PrinterConnection =
	| { kind: "network"; host: string; protocol: NetProtocol; port?: number; queue?: string }
	/** Detected device (CUPS URI / Windows port) or a manual URI. */
	| { kind: "device"; uri: string };

export interface NewPrinter {
	name: string;
	connection: PrinterConnection;
	/** CUPS model / Windows driver; omit for driverless (IPP Everywhere). */
	driver?: string;
	description?: string;
	location?: string;
	shared: boolean;
}

export interface PrinterEdit {
	name: string;
	description?: string;
	location?: string;
	shared: boolean;
}

export interface PrinterDetails {
	name: string;
	description?: string;
	location?: string;
	connection?: string;
	driver?: string;
	shared: boolean;
	can_rename: boolean;
}

export interface PrinterDevice {
	uri: string;
	kind: string;
	info: string;
}

export interface PrinterDriver {
	id: string;
	name: string;
}

export type JobState = "queued" | "printing" | "paused" | "error" | "completed" | "gone";

export interface QueueJob {
	id: string;
	document: string;
	position: number;
	state: JobState;
	message?: string;
	pages?: number;
	pages_printed?: number;
}

export interface PrinterQueue {
	printer: string;
	status: string;
	message?: string;
	jobs: QueueJob[];
}

/** Live status of a job printed through ix.print. `failed` = ix.print could not send it. */
export interface JobStatus {
	job_id: string;
	printer: string;
	spooler_job?: string;
	state: JobState | "failed" | "unknown";
	position?: number;
	ahead?: number;
	message?: string;
	printer_status?: string;
	printer_message?: string;
	remote?: string;
	remote_unreachable?: boolean;
}

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

export interface ImportItem {
	name: string;
	settings: Preset;
	exists: boolean;
	printer_missing: boolean;
}

export interface ImportPreview {
	items: ImportItem[];
	default_printer?: string;
	default_printer_missing: boolean;
	default_preset?: string;
}

export type ImportAction = "overwrite" | "skip" | "rename";

export interface ImportDecision {
	name: string;
	action: ImportAction;
	new_name?: string;
	/** "" clears the preset's printer so the default printer is used. */
	printer?: string;
}

export interface ImportOptions {
	replace_all: boolean;
	include_defaults: boolean;
	decisions: ImportDecision[];
}

export interface HistoryDay {
	date: string;
	total: number;
	failed: number;
}

export interface HistoryEntry {
	id: string;
	time: string;
	source: "http" | "app";
	client?: string;
	/** Name of the API token the request used. */
	token?: string;
	file_name: string;
	preset?: string;
	printer: string;
	spooler_job?: string;
	sheets: number;
	settings: Record<string, unknown>;
	success: boolean;
	error?: string;
	duration_ms: number;
	/** Delivery of the result callback, when the client asked for one. */
	callback?: CallbackStatus;
	remote?: { remote_id: string; remote_name: string; job_id: string };
}

export interface CallbackStatus {
	url: string;
	reference?: string;
	state: "watching" | "retrying" | "delivered" | "failed" | "refused";
	/** Job outcome sent in the callback (completed, failed, …). */
	result?: string;
	attempts: number;
	http_status?: number;
	error?: string;
	time: string;
}

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

export interface HistoryPage {
	entries: HistoryEntry[];
	total: number;
	failed: number;
	page: number;
	pages: number;
	per_page: number;
}

export interface DiagnosticEntry {
	time: string;
	level: string;
	source: string;
	message: string;
	details?: string;
}

export interface JobResult {
	job_id: string;
	printer: string;
	preset?: string;
	sheets: number;
	spooler_job?: string;
}

export const api = {
	/** null when the computer has no network connection. */
	localIp: () => invoke<string | null>("this_local_ip"),
	/** Opens the /docs page in its own window, or in the browser when `external`. */
	openDocs: (external = false) => invoke<void>("open_docs", { external }),
	os: () => invoke<string>("os_detected"),
	engineStatus: () => invoke<void>("engine_status"),
	/** Rejects with the reason when the HTTP server is not running. */
	serverStatus: () => invoke<void>("server_status"),
	bridgeEnabled: () => invoke<boolean>("bridge_enabled"),
	setBridgeEnabled: (enabled: boolean) => invoke<Config>("set_bridge_enabled", { enabled }),
	setServerPort: (port: number) => invoke<Config>("set_server_port", { port }),
	printers: () => invoke<PrinterView[]>("get_printers"),
	remotes: () => invoke<RemoteView[]>("remote_list"),
	remoteAdd: (name: string, url: string, token: string, preset?: string) =>
		invoke<RemoteView[]>("remote_add", { name, url, token, preset: preset ?? null }),
	remoteUpdate: (id: string, name: string, url: string, token?: string, preset?: string) =>
		invoke<RemoteView[]>("remote_update", { id, name, url, token: token || null, preset: preset ?? null }),
	remoteRemove: (id: string) => invoke<RemoteView[]>("remote_remove", { id }),
	remoteTest: (id: string) => invoke<RemoteView>("remote_test", { id }),
	queue: (printer: string) => invoke<PrinterQueue>("get_queue", { printer }),
	jobStatus: (jobId: string) => invoke<JobStatus>("job_status", { jobId }),
	printerDevices: () => invoke<PrinterDevice[]>("printer_devices"),
	printerDrivers: (query: string) => invoke<PrinterDriver[]>("printer_drivers", { query }),
	printerDetails: (name: string) => invoke<PrinterDetails>("printer_details", { name }),
	printerAdd: (printer: NewPrinter) => invoke<void>("printer_add", { printer }),
	printerUpdate: (name: string, edit: PrinterEdit) => invoke<Config>("printer_update", { name, edit }),
	printerRemove: (name: string) => invoke<Config>("printer_remove", { name }),
	printerSetOsDefault: (name: string) => invoke<void>("printer_set_os_default", { name }),
	capabilities: (printer: string) => invoke<Capabilities>("get_capabilities", { printer }),
	config: () => invoke<Config>("get_config"),
	setDefaultPrinter: (printer: string | null) => invoke<Config>("set_default_printer", { printer }),
	setDefaultPreset: (name: string | null) => invoke<Config>("set_default_preset", { name }),
	savePreset: (name: string, preset: Preset, originalName?: string) =>
		invoke<Config>("save_preset", { name, preset, originalName: originalName ?? null }),
	deletePreset: (name: string) => invoke<Config>("delete_preset", { name }),
	/** PNG data URL rendered by PDFium, exactly as it prints. */
	previewTemplate: (template: Template, data: TemplateRecord, pxPerMm: number) =>
		invoke<string>("preview_template", { template, data, pxPerMm }),
	/** Real pagination with the sample data repeated `rows` times: up to 6 page images + total pages. */
	previewPrint: (template: Template, rows: number, pxPerMm: number) =>
		invoke<[string[], number]>("preview_print", { template, rows, pxPerMm }),

	setHistoryRetention: (days: number) => invoke<Config>("set_history_retention", { days }),
	/** Returns the plain token — the only time it is available. */
	createToken: (name: string) => invoke<{ token: string; config: Config }>("create_token", { name }),
	revokeToken: (id: string) => invoke<Config>("revoke_token", { id }),
	setRequireToken: (required: boolean) => invoke<Config>("set_require_token", { required }),
	setAllowedOrigins: (origins: string[]) => invoke<Config>("set_allowed_origins", { origins }),
	tokenUsage: () => invoke<Record<string, TokenUse>>("token_usage"),
	exportPresets: (path: string, names: string[] | null, includeDefaults: boolean) =>
		invoke<number>("export_presets", { path, names, includeDefaults }),
	previewImport: (path: string) => invoke<ImportPreview>("preview_import", { path }),
	applyImport: (path: string, options: ImportOptions) =>
		invoke<{ config: Config; imported: string[] }>("apply_import", { path, options }),
	historyDays: () => invoke<HistoryDay[]>("history_days"),
	historyPage: (date: string, page: number, perPage: number, failedOnly: boolean) =>
		invoke<HistoryPage>("history_page", { date, page, perPage, failedOnly }),
	errorLog: () => invoke<DiagnosticEntry[]>("error_log"),
	recordError: (source: string, message: string, details?: string) =>
		invoke<void>("record_error", { source, message, details: details ?? null }),
	clearErrorLog: () => invoke<void>("clear_error_log"),
	openSystemLogs: () => invoke<void>("open_system_logs"),
	testPrint: (printer: string | null, preset: string | null, pages = 1) =>
		invoke<JobResult>("test_print", { printer, preset, pages }),
};

/** False when the page is opened in a normal browser instead of the ix.print app. */
export const isInApp = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Tauri rejects with the Rust error string; normalise anything else. */
export function errorText(e: unknown): string {
	return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}

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
