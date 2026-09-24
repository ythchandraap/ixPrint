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


/** Print settings plus an optional design; with a design, clients send only data (JSON). */
export type Preset = PrintSettings & { design?: Template };

export interface Config {
	version: number;
	server_port: number;
	require_token: boolean;
	bridge_enabled: boolean;
	lan_access: boolean;
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
