<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { ArrowDown, ArrowUp, Barcode, ClipboardPaste, Copy, Image as ImageIcon, Minus, QrCode, Square, Trash2, Type } from "lucide-vue-next";
import {
	api,
	errorText,
	templateAutoFields,
	templateFields,
	type Template,
	type TemplateElement,
} from "../api";
import AutocompleteField from "./AutocompleteField.vue";

const props = defineProps<{
	/** Name of the preset this design belongs to (for the example request). */
	presetName: string;
	template: Template;
	localIp: string;
	serverPort: number;
}>();

const footerOptions = [
	{ value: "last_page", label: "At the end only (totals, signature)" },
	{ value: "every_page", label: "Every page" },
];
const alignOptions = [
	{ value: "left", label: "Left" },
	{ value: "center", label: "Centre" },
	{ value: "right", label: "Right" },
];
const valignOptions = [
	{ value: "top", label: "Top" },
	{ value: "middle", label: "Centre" },
	{ value: "bottom", label: "Bottom" },
];
const fitOptions = [
	{ value: "contain", label: "Fit (keep ratio)" },
	{ value: "stretch", label: "Fill box" },
];

const emit = defineEmits<{
	/** The edited design; the preset form saves it together with the preset. */
	(e: "apply", design: Template): void;
	(e: "cancel"): void;
}>();

const t = reactive<Template>(JSON.parse(JSON.stringify(props.template)));
if (!t.layout) t.layout = { mode: "label" };

// ---- layout (label / list) -------------------------------------------------
const list = computed(() => (t.layout?.mode === "list" ? t.layout : null));
/** Editor canvas height: the page, or the three list bands stacked. */
const canvasH = computed(() =>
	list.value ? list.value.header_mm + list.value.row_mm + list.value.footer_mm : t.height_mm,
);

function setMode(mode: "label" | "list") {
	if (mode === "label") {
		t.layout = { mode: "label" };
	} else if (!list.value) {
		const header = snap(Math.min(20, t.height_mm / 4));
		t.layout = { mode: "list", header_mm: header, row_mm: 10, footer_mm: 15, footer: "last_page", roll: false };
	}
}

type BandName = "header" | "row" | "footer";
/** Band of an element in a list design, from its top edge. */
function bandOf(el: TemplateElement): BandName {
	const l = list.value;
	if (!l) return "row";
	const top = el.type === "line" ? Math.min(el.y1, el.y2) : el.y;
	return top < l.header_mm - 0.001 ? "header" : top < l.header_mm + l.row_mm - 0.001 ? "row" : "footer";
}

// ---- print preview (real pagination) ------------------------------------------
const previewRows = ref(12);
const printPages = ref<string[]>([]);
const printTotal = ref(0);
const printError = ref("");
const printBusy = ref(false);
async function showPrintPreview() {
	printBusy.value = true;
	printError.value = "";
	try {
		const rows = list.value ? previewRows.value : Math.min(previewRows.value, 6);
		const [pages, total] = await api.previewPrint(JSON.parse(JSON.stringify(t)), rows, 3);
		[printPages.value, printTotal.value] = [pages, total];
	} catch (e) {
		printError.value = errorText(e);
	} finally {
		printBusy.value = false;
	}
}
const selected = ref<number | null>(null);
const copiedElement = ref<TemplateElement | null>(null);
const pasteCount = ref(0);
const preview = ref("");
const previewError = ref("");

const SNAP = 0.5;
const snap = (v: number) => Math.round(v / SNAP) * SNAP;
const newId = () => Math.random().toString(36).slice(2, 10);

// ---- canvas scale -------------------------------------------------------
const holder = ref<HTMLElement | null>(null);
const holderWidth = ref(360);
let observer: ResizeObserver | undefined;
let resizeFrame: number | undefined;
onMounted(() => {
	observer = new ResizeObserver(([entry]) => {
		const width = entry.contentRect.width;
		if (resizeFrame !== undefined) cancelAnimationFrame(resizeFrame);
		resizeFrame = requestAnimationFrame(() => {
			resizeFrame = undefined;
			if (Math.abs(holderWidth.value - width) > 0.25) holderWidth.value = width;
		});
	});
	if (holder.value) observer.observe(holder.value);
});
onBeforeUnmount(() => {
	observer?.disconnect();
	if (resizeFrame !== undefined) cancelAnimationFrame(resizeFrame);
});

/** Screen pixels per mm: fit the width, keep tall labels within ~560 px. */
const scale = computed(() =>
	Math.max(1, Math.min(holderWidth.value / t.width_mm, 560 / canvasH.value, 14)),
);

// ---- live preview (PDFium render) --------------------------------------
let previewTimer: number | undefined;
let previewSeq = 0;
watch(
	() => JSON.stringify(t),
	() => {
		clearTimeout(previewTimer);
		previewTimer = window.setTimeout(async () => {
			const seq = ++previewSeq;
			const px = Math.min(16, Math.max(4, scale.value * (window.devicePixelRatio || 1)));
			try {
				const url = await api.previewTemplate(JSON.parse(JSON.stringify(t)), { ...t.sample }, px);
				if (seq === previewSeq) [preview.value, previewError.value] = [url, ""];
			} catch (e) {
				if (seq === previewSeq) previewError.value = errorText(e);
			}
		}, 150);
	},
	{ immediate: true },
);

// ---- geometry -----------------------------------------------------------
function bbox(el: TemplateElement) {
	switch (el.type) {
		case "line":
			return {
				x: Math.min(el.x1, el.x2),
				y: Math.min(el.y1, el.y2),
				w: Math.max(Math.abs(el.x2 - el.x1), 0.8),
				h: Math.max(Math.abs(el.y2 - el.y1), 0.8),
			};
		case "qr":
			return { x: el.x, y: el.y, w: el.size, h: el.size };
		default:
			return { x: el.x, y: el.y, w: el.w, h: el.h };
	}
}

function moveBy(el: TemplateElement, dx: number, dy: number) {
	if (el.type === "line") {
		el.x1 = snap(el.x1 + dx);
		el.x2 = snap(el.x2 + dx);
		el.y1 = snap(el.y1 + dy);
		el.y2 = snap(el.y2 + dy);
	} else {
		el.x = snap(el.x + dx);
		el.y = snap(el.y + dy);
	}
}

type Drag =
	| { kind: "move"; index: number; startX: number; startY: number; origin: TemplateElement }
	| { kind: "resize"; index: number; startX: number; startY: number; origin: TemplateElement }
	| { kind: "end"; index: number; end: 1 | 2; startX: number; startY: number; origin: TemplateElement };
let drag: Drag | null = null;

const clone = (el: TemplateElement) => JSON.parse(JSON.stringify(el)) as TemplateElement;

function startDrag(e: PointerEvent, index: number, kind: "move" | "resize" | "end1" | "end2") {
	e.preventDefault();
	e.stopPropagation();
	selected.value = index;
	const origin = clone(t.elements[index]);
	const base = { index, startX: e.clientX, startY: e.clientY, origin };
	drag =
		kind === "end1" || kind === "end2"
			? { kind: "end", end: kind === "end1" ? 1 : 2, ...base }
			: { kind, ...base };
	window.addEventListener("pointermove", onDrag);
	window.addEventListener("pointerup", endDrag, { once: true });
}

function onDrag(e: PointerEvent) {
	if (!drag) return;
	const dx = (e.clientX - drag.startX) / scale.value;
	const dy = (e.clientY - drag.startY) / scale.value;
	const o = drag.origin;
	const el = t.elements[drag.index];
	if (drag.kind === "move") {
		Object.assign(el, clone(o));
		moveBy(el, dx, dy);
	} else if (drag.kind === "end" && el.type === "line" && o.type === "line") {
		if (drag.end === 1) [el.x1, el.y1] = [snap(o.x1 + dx), snap(o.y1 + dy)];
		else [el.x2, el.y2] = [snap(o.x2 + dx), snap(o.y2 + dy)];
	} else if (drag.kind === "resize") {
		if (el.type === "qr" && o.type === "qr") el.size = Math.max(2, snap(o.size + Math.max(dx, dy)));
		else if (el.type !== "line" && el.type !== "qr" && o.type !== "line" && o.type !== "qr") {
			el.w = Math.max(1, snap(o.w + dx));
			el.h = Math.max(1, snap(o.h + dy));
		}
	}
}

function endDrag() {
	drag = null;
	window.removeEventListener("pointermove", onDrag);
}
onBeforeUnmount(endDrag);

function onKey(e: KeyboardEvent) {
	const target = e.target as HTMLElement;
	if (target.closest("input, textarea, select, [contenteditable=true]")) return;
	const modifier = e.metaKey || e.ctrlKey;
	if (modifier && e.key.toLowerCase() === "c" && selected.value !== null) {
		e.preventDefault();
		copySelected();
		return;
	}
	if (modifier && e.key.toLowerCase() === "v" && copiedElement.value) {
		e.preventDefault();
		pasteCopied();
		return;
	}
	if (selected.value === null || !target.closest(".stage")) return;
	const step = e.shiftKey ? 5 : SNAP;
	const moves: Record<string, [number, number]> = {
		ArrowLeft: [-step, 0],
		ArrowRight: [step, 0],
		ArrowUp: [0, -step],
		ArrowDown: [0, step],
	};
	if (moves[e.key]) {
		e.preventDefault();
		moveBy(t.elements[selected.value], ...moves[e.key]);
	} else if (e.key === "Delete" || e.key === "Backspace") {
		e.preventDefault();
		removeSelected();
	}
}

// ---- element operations -------------------------------------------------
/** Adds a shape; images come from pickImage() instead. */
function add(type: Exclude<TemplateElement["type"], "image">) {
	const W = t.width_mm;
	const H = canvasH.value;
	const m = Math.min(2, W / 10);
	// In a list, new elements start in the row band.
	const y0 = list.value ? list.value.header_mm + 0.5 : m;
	const el: TemplateElement =
		type === "text"
			? { type, id: newId(), x: m, y: y0, w: W - 2 * m, h: list.value ? Math.max(1, list.value.row_mm - 1) : Math.min(8, H - 2 * m), text: "Text {{kolom}}", size: 10, bold: false, align: "left", valign: "top", wrap: true, fit: true }
			: type === "line"
				? { type, id: newId(), x1: m, y1: snap(H / 2), x2: W - m, y2: snap(H / 2), width: 0.75 }
				: type === "rect"
					? { type, id: newId(), x: m, y: m, w: W - 2 * m, h: H - 2 * m, stroke: 0.75, fill: false }
					: type === "barcode"
						? { type, id: newId(), x: m, y: m, w: Math.min(50, W - 2 * m), h: Math.min(12, H - 2 * m), value: "{{kode}}", show_text: true }
						: { type, id: newId(), x: m, y: m, size: Math.min(20, W - 2 * m, H - 2 * m), value: "{{kode}}" };
	t.elements.push(el);
	if (el.type === "text") ensureSample("kolom");
	else if (el.type === "barcode" || el.type === "qr") ensureSample("kode");
	selected.value = t.elements.length - 1;
}

function removeSelected() {
	if (selected.value === null) return;
	t.elements.splice(selected.value, 1);
	selected.value = null;
}

function copySelected() {
	if (selected.value === null) return;
	copiedElement.value = clone(t.elements[selected.value]);
	pasteCount.value = 0;
}

function pasteCopied() {
	if (!copiedElement.value) return;
	const copy = clone(copiedElement.value);
	copy.id = newId();
	pasteCount.value += 1;
	moveBy(copy, 2 * pasteCount.value, 2 * pasteCount.value);
	t.elements.push(copy);
	selected.value = t.elements.length - 1;
}

function duplicateSelected() {
	copySelected();
	pasteCopied();
}

/** Moves the selected element up/down the stacking order (later = on top). */
function reorderAt(i: number, dir: -1 | 1) {
	const j = i + dir;
	if (j < 0 || j >= t.elements.length) return;
	[t.elements[i], t.elements[j]] = [t.elements[j], t.elements[i]];
	if (selected.value === i) selected.value = j;
	else if (selected.value === j) selected.value = i;
}

function selectAndReorder(index: number, dir: -1 | 1) {
	selected.value = index;
	reorderAt(index, dir);
}

const current = computed(() => (selected.value === null ? null : t.elements[selected.value]));
function layerLabel(el: TemplateElement) {
	switch (el.type) {
		case "text": return `Text · ${el.text.replace(/\s+/g, " ").trim() || "Empty"}`;
		case "line": return "Line";
		case "rect": return "Rectangle";
		case "barcode": return `Barcode · ${el.value}`;
		case "qr": return `QR code · ${el.value}`;
		case "image": return `Image · ${el.src.startsWith("{{") ? el.src.slice(2, -2) : "Embedded image"}`;
	}
	return "Element";
}
const layerRows = computed(() =>
	t.elements.map((element, index) => ({ element, index, label: layerLabel(element) })).reverse(),
);

// ---- images ---------------------------------------------------------------
const MAX_IMAGE_PX = 1600;
const fileInput = ref<HTMLInputElement | null>(null);
/** Element index whose image is being replaced; null = add a new image. */
let replacing: number | null = null;
const imageError = ref("");

function pickImage(index: number | null) {
	replacing = index;
	imageError.value = "";
	fileInput.value?.click();
}

/**
 * Turns any image the system can display (PNG, JPEG, GIF, WebP, BMP, SVG, AVIF,
 * HEIC on macOS, ...) into a PNG/JPEG data URL, scaled down to MAX_IMAGE_PX.
 */
function toDataUrl(file: File): Promise<{ url: string; width: number; height: number }> {
	return new Promise((resolve, reject) => {
		const objectUrl = URL.createObjectURL(file);
		const img = new Image();
		img.onload = () => {
			// SVGs without width/height report 0: give them a sensible raster size.
			let w = img.naturalWidth || 1024;
			let h = img.naturalHeight || 1024;
			const s = Math.min(1, MAX_IMAGE_PX / Math.max(w, h));
			w = Math.max(1, Math.round(w * s));
			h = Math.max(1, Math.round(h * s));
			const canvas = document.createElement("canvas");
			[canvas.width, canvas.height] = [w, h];
			const ctx = canvas.getContext("2d");
			if (!ctx) return reject("Canvas is unavailable");
			ctx.drawImage(img, 0, 0, w, h);
			URL.revokeObjectURL(objectUrl);
			// Photos stay small as JPEG; everything else keeps transparency as PNG.
			const url = file.type === "image/jpeg" ? canvas.toDataURL("image/jpeg", 0.9) : canvas.toDataURL("image/png");
			resolve({ url, width: w, height: h });
		};
		img.onerror = () => {
			URL.revokeObjectURL(objectUrl);
		reject(`"${file.name}" cannot be opened as an image: the file is damaged or its format is not supported on this computer`);
		};
		img.src = objectUrl;
	});
}

async function onImageFile(event: Event) {
	const input = event.target as HTMLInputElement;
	const file = input.files?.[0];
	input.value = "";
	if (!file) return;
	try {
		const { url, width, height } = await toDataUrl(file);
		const target = replacing === null ? null : t.elements[replacing];
		if (target && target.type === "image") {
			target.src = url;
			return;
		}
		// New image: 40% of the design width, keeping the aspect ratio, inside the design.
		const W = t.width_mm;
		const H = canvasH.value;
		let w = snap(Math.max(5, Math.min(W * 0.4, W - 4)));
		let h = snap((w * height) / width);
		if (h > H - 4) {
			h = snap(Math.max(5, H - 4));
			w = snap((h * width) / height);
		}
		t.elements.push({ type: "image", id: newId(), x: 2, y: 2, w, h, src: url, fit: "contain", grayscale: false });
		selected.value = t.elements.length - 1;
	} catch (e) {
		imageError.value = errorText(e);
	}
}

/** Switches an image between a fixed picture and a per-record {{field}}. */
function useImageField(el: TemplateElement, field: string) {
	if (el.type !== "image") return;
	const f = field.trim().replace(/[^A-Za-z0-9_.-]/g, "_");
	el.src = f ? `{{${f}}}` : "";
}

// ---- data fields ----------------------------------------------------------
const fields = computed(() => templateFields(t));
function ensureSample(field: string) {
	if (!(field in t.sample) || !t.sample[field]) t.sample[field] = "Contoh";
}
/** {{no}}, {{halaman}}... used by the design: filled automatically unless given. */
const autoFields = computed(() => templateAutoFields(t));
const AUTO_HINT: Record<string, string> = {
	no: "item number (1, 2, 3…)",
	halaman: "page number",
	total_halaman: "total pages",
	jumlah_data: "item count",
};
watch(fields, (list) => {
	for (const f of list) if (!(f in t.sample)) t.sample[f] = "Contoh";
}, { immediate: true });
function insertField(field: string) {
	const el = current.value;
	if (!el) return;
	if (el.type === "text") {
		el.text += `{{${field}}}`;
		ensureSample(field);
	} else if (el.type === "barcode" || el.type === "qr") {
		el.value = `{{${field}}}`;
		ensureSample(field);
	}
}
const newField = ref("");
function addField() {
	const f = newField.value.trim().replace(/[^A-Za-z0-9_.-]/g, "_");
	if (!f) return;
	insertField(f);
	newField.value = "";
}

const QUICK_SIZES: [string, number, number][] = [
	["58 × 40", 58, 40],
	["80 × 50", 80, 50],
	["60 × 75", 60, 75],
	["100 × 50", 100, 50],
	["100 × 150", 100, 150],
];

const TYPE_LABEL: Record<TemplateElement["type"], string> = {
	text: "Text",
	line: "Line",
	rect: "Rectangle",
	barcode: "Barcode",
	qr: "QR",
	image: "Image",
};

/** Fields per band, so the example request puts each in the right place. */
const bandFields = computed(() => {
	const shared = new Set<string>();
	const row = new Set<string>();
	for (const el of t.elements) {
		const one: Template = { width_mm: 1, height_mm: 1, elements: [el], sample: {} };
		for (const f of templateFields(one)) (list.value && bandOf(el) !== "row" ? shared : row).add(f);
	}
	return { shared: [...shared].sort(), row: [...row].sort() };
});

const exampleBody = computed(() => {
	const pick = (names: string[]) => Object.fromEntries(names.map((f) => [f, t.sample[f] || "…"]));
	const body: Record<string, unknown> = { preset: props.presetName || "nama-preset" };
	if (bandFields.value.shared.length) body.header = pick(bandFields.value.shared);
	// Always an array: one object per label / per row.
	const one = pick(bandFields.value.row);
	body.data = list.value ? [one, one] : [one];
	return JSON.stringify(body, null, 2);
});

function save() {
	const payload: Template = JSON.parse(JSON.stringify(t));
	// Keep sample values only for fields the design still uses.
	payload.sample = Object.fromEntries(fields.value.map((f) => [f, payload.sample[f] ?? ""]));
	// Automatic fields keep a sample value only when the user typed one.
	for (const f of autoFields.value) if (t.sample[f]) payload.sample[f] = t.sample[f];
	emit("apply", payload);
}
</script>

<template>
	<form class="stack" @submit.prevent="save" @keydown="onKey">
		<h3 class="section-title">Design{{ presetName ? ` preset "${presetName}"` : "" }}</h3>
		<p class="hint">The preset paper size follows the design. Printer, X/Y offsets, copies and other settings come from the preset.</p>
		<div class="segmented">
			<button type="button" :class="{ on: !list }" @click="setMode('label')">Label · 1 item = 1 page</button>
			<button type="button" :class="{ on: !!list }" @click="setMode('list')">List · multiple items per page</button>
		</div>
		<div class="grid2">
			<label class="field">
				<span>Page width (mm)</span>
				<input v-model.number="t.width_mm" type="number" min="5" max="2000" step="any" required />
			</label>
			<label v-if="!list?.roll" class="field">
				<span>Page height (mm)</span>
				<input v-model.number="t.height_mm" type="number" min="5" max="2000" step="any" required />
			</label>
		</div>
		<fieldset v-if="list">
			<legend>Bands</legend>
			<p class="hint">
				The header is printed at the top of each page, the row repeats for each item, and the footer
				{{ list.footer === "every_page" ? "appears at the bottom of each page" : "appears once after the last row" }}.
				Elements are assigned to a band according to their canvas position.
			</p>
			<div class="grid4">
				<label class="field"><span>Header (mm)</span><input v-model.number="list.header_mm" type="number" min="0" step="any" /></label>
				<label class="field"><span>Row (mm)</span><input v-model.number="list.row_mm" type="number" min="1" step="any" /></label>
				<label class="field"><span>Footer (mm)</span><input v-model.number="list.footer_mm" type="number" min="0" step="any" /></label>
			</div>
			<div class="grid2">
				<label class="field"><span>Footer</span>
			<AutocompleteField v-model="list.footer" :options="footerOptions" aria-label="Footer position" />
				</label>
				<label class="row tight check"><input v-model="list.roll" type="checkbox" /> Roll/receipt: height follows content</label>
			</div>
		</fieldset>
		<div v-if="!list?.roll" class="row wrap">
			<button
				v-for="[label, w, h] in QUICK_SIZES"
				:key="label"
				type="button"
				class="ghost small"
				@click="[t.width_mm, t.height_mm] = [w, h]"
			>
				{{ label }}
			</button>
		</div>

		<div class="row wrap toolbar">
			<span class="meta">Add:</span>
			<button type="button" class="ghost small" @click="add('text')"><Type :size="14" /> Text</button>
			<button type="button" class="ghost small" @click="add('line')"><Minus :size="14" /> Line</button>
			<button type="button" class="ghost small" @click="add('rect')"><Square :size="14" /> Rectangle</button>
			<button type="button" class="ghost small" @click="add('barcode')"><Barcode :size="14" /> Barcode</button>
			<button type="button" class="ghost small" @click="add('qr')"><QrCode :size="14" /> QR</button>
			<button type="button" class="ghost small" @click="pickImage(null)"><ImageIcon :size="14" /> Image</button>
			<input ref="fileInput" type="file" accept="image/*,.svg,.heic,.heif,.avif,.tif,.tiff,.ico" hidden @change="onImageFile" />
		</div>
		<p v-if="imageError" class="error">⚠ {{ imageError }}</p>

		<!-- WYSIWYG canvas: PDFium render underneath, draggable element boxes on top -->
		<div ref="holder" class="holder">
			<div
				class="stage"
				tabindex="0"
				:style="{ width: `${t.width_mm * scale}px`, height: `${canvasH * scale}px` }"
				@pointerdown.self="selected = null"
			>
				<img v-if="preview" :src="preview" class="render" alt="" draggable="false" />
				<template v-if="list">
					<div class="band-label" :style="{ top: '0px' }">HEADER</div>
					<div class="band-line" :style="{ top: `${list.header_mm * scale}px` }"></div>
					<div class="band-label" :style="{ top: `${list.header_mm * scale}px` }">ROW (repeated)</div>
					<div class="band-line" :style="{ top: `${(list.header_mm + list.row_mm) * scale}px` }"></div>
					<div class="band-label" :style="{ top: `${(list.header_mm + list.row_mm) * scale}px` }">FOOTER</div>
				</template>
				<template v-for="(el, i) in t.elements" :key="el.id || i">
					<div
						:class="['box', { on: selected === i }]"
						:style="{
							left: `${bbox(el).x * scale}px`,
							top: `${bbox(el).y * scale}px`,
							width: `${bbox(el).w * scale}px`,
							height: `${bbox(el).h * scale}px`,
						}"
						:title="TYPE_LABEL[el.type]"
						@pointerdown="startDrag($event, i, 'move')"
					>
						<span
							v-if="selected === i && el.type !== 'line'"
							class="handle br"
							@pointerdown="startDrag($event, i, 'resize')"
						></span>
					</div>
					<template v-if="selected === i && el.type === 'line'">
						<span
							class="handle end"
							:style="{ left: `${el.x1 * scale}px`, top: `${el.y1 * scale}px` }"
							@pointerdown="startDrag($event, i, 'end1')"
						></span>
						<span
							class="handle end"
							:style="{ left: `${el.x2 * scale}px`, top: `${el.y2 * scale}px` }"
							@pointerdown="startDrag($event, i, 'end2')"
						></span>
					</template>
				</template>
			</div>
		</div>
		<p v-if="previewError" class="error">⚠ {{ previewError }}</p>
		<p class="hint">Move elements with the mouse or arrow keys (Shift = 5 mm). The preview is the actual print output.</p>

		<fieldset class="layers-panel">
			<legend>Layers · {{ t.elements.length }}</legend>
			<div class="row wrap layers-toolbar">
				<span class="muted grow">Top layer appears first. Select a row to edit it.</span>
				<button type="button" class="ghost small" :disabled="selected === null" title="Copy selected element (⌘/Ctrl+C)" @click="copySelected">
					<Copy :size="14" /> Copy
				</button>
				<button type="button" class="ghost small" :disabled="!copiedElement" title="Paste element (⌘/Ctrl+V)" @click="pasteCopied">
					<ClipboardPaste :size="14" /> Paste
				</button>
			</div>
			<div v-if="layerRows.length" class="layers-list">
				<div v-for="layer in layerRows" :key="layer.element.id" class="layer-row">
					<button
						type="button"
						:class="['layer-select', { on: selected === layer.index }]"
						:aria-pressed="selected === layer.index"
						:title="layer.label"
						@click="selected = layer.index"
					>
						<span class="layer-index">{{ t.elements.length - layer.index }}</span>
						<span class="layer-label">{{ layer.label }}</span>
					</button>
					<div class="row tight layer-order-actions">
						<button type="button" class="icon" title="Move forward" :aria-label="`Move ${layer.label} forward`" :disabled="layer.index === t.elements.length - 1" @click="selectAndReorder(layer.index, 1)">
							<ArrowUp :size="14" />
						</button>
						<button type="button" class="icon" title="Move backward" :aria-label="`Move ${layer.label} backward`" :disabled="layer.index === 0" @click="selectAndReorder(layer.index, -1)">
							<ArrowDown :size="14" />
						</button>
					</div>
				</div>
			</div>
			<p v-else class="muted">No elements yet. Add an element above to start the design.</p>
			<p class="meta">Copy and paste work inside this design. Pasted copies are offset slightly.</p>
		</fieldset>

		<!-- properties of the selected element -->
		<fieldset v-if="current">
			<legend>{{ TYPE_LABEL[current.type] }}</legend>
			<template v-if="current.type === 'text'">
				<label class="field">
					<span>Content (use {{ "{" + "{field}" + "}" }} for data)</span>
					<textarea v-model="current.text" rows="2"></textarea>
				</label>
				<div class="grid4">
					<label class="field"><span>Ukuran (pt)</span><input v-model.number="current.size" type="number" min="2" max="300" step="any" /></label>
					<label class="field"><span>Alignment</span>
						<AutocompleteField v-model="current.align" :options="alignOptions" aria-label="Text alignment" />
					</label>
					<label class="field"><span>Vertical</span>
						<AutocompleteField v-model="current.valign" :options="valignOptions" aria-label="Vertical alignment" />
					</label>
					<label class="field"><span>&nbsp;</span>
						<span class="row tight"><input v-model="current.bold" type="checkbox" /> Bold</span>
					</label>
				</div>
				<div class="row wrap">
					<label class="row tight check"><input v-model="current.wrap" type="checkbox" /> Wrap text</label>
					<label class="row tight check"><input v-model="current.fit" type="checkbox" /> Shrink to fit</label>
				</div>
			</template>
			<template v-if="current.type === 'barcode' || current.type === 'qr'">
				<label class="field">
					<span>Value ({{ current.type === "barcode" ? "Code128, ASCII" : "text/URL" }})</span>
					<input v-model="current.value" />
				</label>
				<label v-if="current.type === 'barcode'" class="row tight check">
					<input v-model="current.show_text" type="checkbox" /> Show text below barcode
				</label>
			</template>
			<template v-if="current.type === 'image'">
				<div class="row wrap">
					<img v-if="current.src.startsWith('data:')" :src="current.src" class="image-thumb" alt="" />
					<span v-else class="meta">From data field: <code>{{ current.src || "—" }}</code></span>
					<button type="button" class="ghost small" @click="pickImage(selected)">Replace image…</button>
				</div>
				<div class="grid2">
					<label class="field"><span>Placement</span>
						<AutocompleteField v-model="current.fit" :options="fitOptions" aria-label="Image placement" />
					</label>
					<label class="field"><span>Data field (optional)</span>
						<input
							:value="current.src.startsWith('{{') ? current.src.replace(/[{}]/g, '').trim() : ''"
							placeholder="e.g. photo — image per item"
							@change="useImageField(current, ($event.target as HTMLInputElement).value)"
						/>
					</label>
				</div>
				<label class="row tight check"><input v-model="current.grayscale" type="checkbox" /> Black and white (clearer on thermal printers)</label>
				<p class="hint">
					Any format this computer can open (PNG, JPG, SVG, WebP, GIF, BMP, AVIF…). From a data field, the web app sends
					the image as a data URL/base64 (PNG, JPEG, GIF, BMP, WebP, TIFF, ICO).
				</p>
			</template>
			<template v-if="current.type === 'rect'">
				<div class="row wrap">
					<label class="field"><span>Border (pt, 0 = none)</span><input v-model.number="current.stroke" type="number" min="0" step="any" /></label>
					<label class="row tight check"><input v-model="current.fill" type="checkbox" /> Black fill</label>
				</div>
			</template>

			<div v-if="current.type === 'line'" class="grid4">
				<label class="field"><span>X1</span><input v-model.number="current.x1" type="number" step="any" /></label>
				<label class="field"><span>Y1</span><input v-model.number="current.y1" type="number" step="any" /></label>
				<label class="field"><span>X2</span><input v-model.number="current.x2" type="number" step="any" /></label>
				<label class="field"><span>Y2</span><input v-model.number="current.y2" type="number" step="any" /></label>
				<label class="field"><span>Width (pt)</span><input v-model.number="current.width" type="number" min="0.1" step="any" /></label>
			</div>
			<div v-else-if="current.type === 'qr'" class="grid4">
				<label class="field"><span>X</span><input v-model.number="current.x" type="number" step="any" /></label>
				<label class="field"><span>Y</span><input v-model.number="current.y" type="number" step="any" /></label>
				<label class="field"><span>Ukuran</span><input v-model.number="current.size" type="number" min="2" step="any" /></label>
			</div>
			<div v-else class="grid4">
				<label class="field"><span>X</span><input v-model.number="current.x" type="number" step="any" /></label>
				<label class="field"><span>Y</span><input v-model.number="current.y" type="number" step="any" /></label>
				<label class="field"><span>Width</span><input v-model.number="current.w" type="number" min="1" step="any" /></label>
				<label class="field"><span>Height</span><input v-model.number="current.h" type="number" min="1" step="any" /></label>
			</div>

			<div v-if="current.type === 'text' || current.type === 'barcode' || current.type === 'qr'" class="row wrap">
				<span class="meta">Insert field:</span>
				<button v-for="f in fields" :key="f" type="button" class="ghost small" @click="insertField(f)">{{ f }}</button>
				<input v-model="newField" class="field-input" placeholder="new field" @keydown.enter.prevent="addField" />
				<button type="button" class="ghost small" @click="addField">+</button>
			</div>

			<div class="row wrap">
				<button type="button" class="icon" title="Duplicate" @click="duplicateSelected"><Copy :size="16" /></button>
				<span class="grow"></span>
				<button type="button" class="icon danger" title="Remove element" @click="removeSelected"><Trash2 :size="16" /></button>
			</div>
		</fieldset>
		<p v-else class="hint">Click an element in the preview to edit it.</p>

		<fieldset>
			<legend>Sample data</legend>
			<p class="hint">Enter example values to see dynamic text in the design preview and test print. New fields start with “Contoh”.</p>
			<p v-if="!fields.length" class="hint">No fields yet. Write {{ "{" + "{field}" + "}" }} in text, barcode or QR content.</p>
			<template v-if="autoFields.length">
				<p class="hint">Automatic fields — leave blank for ix.print to fill, or enter your own value (the web app may also send it):</p>
				<div class="grid2">
					<label v-for="f in autoFields" :key="f" class="field">
						<span>{{ f }}</span>
						<input v-model="t.sample[f]" :placeholder="`automatic: ${AUTO_HINT[f]}`" />
					</label>
				</div>
			</template>
			<p v-else class="hint">
				Available automatic fields: <code>{{ "{" + "{no}" + "}" }}</code>, <code>{{ "{" + "{halaman}" + "}" }}</code>,
				<code>{{ "{" + "{total_halaman}" + "}" }}</code>, <code>{{ "{" + "{jumlah_data}" + "}" }}</code> — their values can also be provided manually.
			</p>
			<div class="grid2">
				<label v-for="f in fields" :key="f" class="field">
					<span>{{ f }}</span>
					<input v-model="t.sample[f]" placeholder="e.g. Product name" />
				</label>
			</div>
		</fieldset>

		<fieldset>
			<legend>Call from a web app</legend>
			<p class="hint">Send a JSON body (plus the token header when required):</p>
			<pre class="settings">POST http://{{ localIp }}:{{ serverPort }}/print
Content-Type: application/json
{{ exampleBody }}</pre>
			<p class="hint">
				<code>"data"</code> is an array: one object = {{ list ? "one row" : "one label" }}.
				<code>"header"</code> (optional) contains values shared by every page{{ list ? " — fields in the Header/Footer bands" : "" }}.
				Other settings go in <code>"settings"</code>, e.g. <code>{"copies": 2}</code>.
			</p>
		</fieldset>

		<fieldset>
			<legend>Print preview</legend>
			<div class="row">
				<label class="row tight grow">
					<input v-model.number="previewRows" class="rows-input" type="number" min="1" max="500" />
					<span class="meta">sample items</span>
				</label>
				<button type="button" class="ghost small" :disabled="printBusy" @click="showPrintPreview">
					{{ printBusy ? "Rendering…" : "Show" }}
				</button>
			</div>
			<p v-if="printError" class="error">{{ printError }}</p>
			<template v-if="printPages.length">
				<p class="meta">{{ printTotal }} page{{ printTotal === 1 ? "" : "s" }}{{ printTotal > printPages.length ? ` (${printPages.length} first shown)` : "" }}</p>
				<div class="pages">
					<figure v-for="(src, i) in printPages" :key="i">
						<img :src="src" alt="" />
						<figcaption class="meta">p. {{ i + 1 }}</figcaption>
					</figure>
				</div>
			</template>
		</fieldset>

		<div class="row end">
			<button type="button" class="ghost" @click="emit('cancel')">Cancel</button>
			<button type="submit" class="primary">Save design &amp; preset</button>
		</div>
	</form>
</template>

<style scoped>
.holder {
	width: 100%;
	overflow: auto;
	display: flex;
	justify-content: center;
	padding: 8px 0;
	background: repeating-conic-gradient(var(--line) 0% 25%, transparent 0% 50%) 50% / 16px 16px;
	border-radius: 8px;
}
.stage {
	position: relative;
	background: #fff;
	box-shadow: 0 1px 4px rgb(0 0 0 / 0.25);
	outline: none;
	flex: none;
	touch-action: none;
}
.render {
	position: absolute;
	inset: 0;
	width: 100%;
	height: 100%;
	pointer-events: none;
	user-select: none;
}
.box {
	position: absolute;
	border: 1px dashed rgb(59 130 246 / 0.45);
	cursor: move;
	box-sizing: border-box;
}
.box.on {
	border: 1.5px solid var(--brand);
	background: rgb(59 130 246 / 0.06);
}
.handle {
	position: absolute;
	width: 10px;
	height: 10px;
	background: var(--brand);
	border: 2px solid #fff;
	border-radius: 50%;
	box-shadow: 0 0 2px rgb(0 0 0 / 0.4);
}
.handle.br {
	right: -6px;
	bottom: -6px;
	cursor: nwse-resize;
}
.handle.end {
	transform: translate(-50%, -50%);
	cursor: crosshair;
	z-index: 2;
}
.toolbar {
	gap: 4px;
}
textarea {
	font: inherit;
	font-size: 14px;
	color: var(--text);
	background: var(--surface);
	border: 1px solid var(--line);
	border-radius: 8px;
	padding: 6px 8px;
	resize: vertical;
}
.image-thumb {
	max-width: 80px;
	max-height: 48px;
	border: 1px solid var(--line);
	border-radius: 4px;
	background: repeating-conic-gradient(var(--line) 0% 25%, #fff 0% 50%) 50% / 8px 8px;
}
.segmented {
	display: flex;
	border: 1px solid var(--line);
	border-radius: 8px;
	overflow: hidden;
}
.segmented button {
	flex: 1;
	min-height: var(--control-height);
	border: none;
	background: var(--surface);
	color: var(--muted);
	padding: 7px 4px;
	font: inherit;
	font-size: 12px;
	cursor: pointer;
}
.segmented button.on {
	background: var(--brand-fill);
	color: #fff;
}
.band-line {
	position: absolute;
	left: 0;
	right: 0;
	border-top: 1px dashed var(--accent);
	pointer-events: none;
}
.band-label {
	position: absolute;
	right: 2px;
	font-size: 9px;
	font-weight: 600;
	font-family: var(--font-mono);
	letter-spacing: 0.04em;
	color: var(--cobalt-deep);
	background: rgb(255 255 255 / 0.85);
	padding: 0 3px;
	pointer-events: none;
	transform: translateY(1px);
}
.rows-input {
	width: 80px !important;
}
.pages {
	display: flex;
	gap: 8px;
	overflow-x: auto;
	padding-bottom: 4px;
}
.pages figure {
	margin: 0;
	flex: none;
	text-align: center;
}
.pages img {
	max-height: 220px;
	border: 1px solid var(--line);
	background: #fff;
	display: block;
}
.field-input {
	width: 110px !important;
}
.layers-toolbar {
	align-items: center;
}
.layers-list {
	display: flex;
	flex-direction: column;
	gap: 3px;
	max-height: 220px;
	overflow-y: auto;
}
.layer-row {
	display: flex;
	align-items: center;
	gap: 4px;
	min-width: 0;
	padding: 2px;
	border: 1px solid var(--line);
	border-radius: var(--r-md);
}
.layer-select {
	display: flex;
	align-items: center;
	gap: 8px;
	flex: 1;
	min-width: 0;
	min-height: 32px;
	padding: 4px 6px;
	border: 0;
	border-radius: var(--r-sm);
	background: transparent;
	color: var(--text);
	font: inherit;
	text-align: left;
	cursor: pointer;
}
.layer-select:hover,
.layer-select.on {
	background: var(--brand-pale);
}
.layer-select.on {
	color: var(--brand);
}
.layer-index {
	display: grid;
	place-items: center;
	width: 20px;
	height: 20px;
	flex: none;
	border-radius: 5px;
	background: var(--bg);
	color: var(--muted);
	font-family: var(--font-mono);
	font-size: 10px;
}
.layer-label {
	min-width: 0;
	overflow: hidden;
	text-overflow: ellipsis;
	white-space: nowrap;
}
.layer-order-actions {
	gap: 0;
}
</style>
