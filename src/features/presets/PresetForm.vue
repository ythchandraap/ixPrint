<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import {
	api,
	errorText,
	type Capabilities,
	type Config,
	type Preset,
	type PrinterView,
	type PrintSettings,
	type Template,
} from "@/shared/api";
import { templateFields } from "@/shared/lib/template";
import AutocompleteField from "@/components/AutocompleteField.vue";
import DesignEditor from "@/features/design-editor/DesignEditor.vue";

const props = defineProps<{
	/** Existing preset name, or undefined for a new preset. */
	name?: string;
	settings: Preset;
	printers: PrinterView[];
	localIp: string;
	serverPort: number;
}>();

const emit = defineEmits<{
	(e: "saved", config: Config): void;
	(e: "cancel"): void;
}>();

// Must match STANDARD_PAPERS in src-tauri/src/settings.rs.
const STANDARD = ["A3", "A4", "A5", "A6", "B5", "Letter", "Legal", "F4"];
const collateOptions = [
	{ value: "", label: "—" },
	{ value: "true", label: "Yes (1,2,3 · 1,2,3)" },
	{ value: "false", label: "No (1,1 · 2,2)" },
];
const orientationOptions = [
	{ value: "", label: "—" },
	{ value: "auto", label: "Auto (follow page)" },
	{ value: "portrait", label: "Portrait / upright" },
	{ value: "landscape", label: "Landscape / rotated 90°" },
];
const alignOptions = [
	{ value: "", label: "—" },
	{ value: "center", label: "Centre" },
	{ value: "top", label: "Top centre" },
	{ value: "top-left", label: "Top left" },
];
const scaleOptions = [
	{ value: "", label: "—" },
	{ value: "shrink", label: "Shrink if needed" },
	{ value: "fit", label: "Fit to paper" },
	{ value: "none", label: "Original size" },
	{ value: "percent", label: "Percentage…" },
];
const duplexOptions = [
	{ value: "", label: "—" },
	{ value: "off", label: "Single-sided" },
	{ value: "long-edge", label: "Long edge" },
	{ value: "short-edge", label: "Short edge" },
];
const colorOptions = [
	{ value: "", label: "—" },
	{ value: "color", label: "Colour" },
	{ value: "grayscale", label: "Greyscale" },
];

/** Form state: "" everywhere means "not set, use the next layer". */
function toForm(s: PrintSettings) {
	const f = {
		name: props.name ?? "",
		printer: s.printer ?? "",
		copies: s.copies?.toString() ?? "",
		collate: s.collate === undefined ? "" : String(s.collate),
		orientation: s.orientation ?? "",
		paperMode: "",
		/** "custom" size in mm; an empty height means roll paper (length follows content). */
		customW: "" as string | number,
		customH: "" as string | number,
		scaleMode: "",
		scalePercent: "100",
		align: s.align ?? "",
		margins: ["", "", "", ""] as (string | number)[],
		offsetX: s.offset_x?.toString() ?? "",
		offsetY: s.offset_y?.toString() ?? "",
		pages: s.pages ?? "",
		duplex: s.duplex ?? "",
		color: s.color ?? "",
		tray: s.tray ?? "",
		dpi: s.dpi?.toString() ?? "",
	};

	const paper = s.paper;
	if (paper) {
		const custom = paper.match(/^([\d.]+)x([\d.]+)mm$/i);
		const roll = paper.match(/^([\d.]+)mm$/i);
		if (paper === "default") f.paperMode = "default";
		else if (STANDARD.includes(paper)) f.paperMode = paper;
		else if (custom) [f.paperMode, f.customW, f.customH] = ["custom", custom[1], custom[2]];
		else if (roll) [f.paperMode, f.customW] = ["custom", roll[1]];
	}

	if (s.scale) {
		const pct = s.scale.match(/^([\d.]+)%$/);
		if (pct) [f.scaleMode, f.scalePercent] = ["percent", pct[1]];
		else f.scaleMode = s.scale;
	}

	if (s.margins) {
		const v = s.margins.split(",").map((x) => x.trim());
		f.margins =
			v.length === 1 ? [v[0], v[0], v[0], v[0]] : v.length === 2 ? [v[0], v[1], v[0], v[1]] : v.slice(0, 4);
	}
	return f;
}

const form = reactive(toForm(props.settings));
const error = ref("");
const saving = ref(false);
const caps = ref<Capabilities | null>(null);
const printerPaperChoice = ref("");
const paperModeOptions = computed(() => [
	{ value: "", label: "—" },
	{ value: "default", label: "Use printer settings" },
	...STANDARD.map((p) => ({ value: p, label: p })),
	{ value: "custom", label: "Custom size (width × length)" },
]);
const printerOptions = computed(() => [
	{ value: "", label: "— default printer —" },
	...props.printers.map((p) => ({ value: p.name, label: p.name })),
	...(form.printer && !props.printers.some((p) => p.name === form.printer)
		? [{ value: form.printer, label: `${form.printer} (not installed)` }]
		: []),
]);
const printerPaperOptions = computed(() => [
	{ value: "", label: "Choose…" },
	...(caps.value?.papers ?? []).map((p, i) => ({
		value: `cap:${i}`,
		label: `${p.name} (${Math.round(p.width_mm)}×${Math.round(p.height_mm)} mm)`,
	})),
]);
const trayOptions = computed(() => [
	{ value: "", label: "—" },
	...(caps.value?.trays ?? []).map((t) => ({ value: t, label: t })),
	...(form.tray && !(caps.value?.trays ?? []).includes(form.tray) ? [{ value: form.tray, label: `${form.tray} (saved)` }] : []),
]);

/** Printer whose capabilities drive the paper and tray lists. */
const effectivePrinter = computed(
	() => form.printer || props.printers.find((p) => p.is_default)?.name || "",
);

watch(
	effectivePrinter,
	async (name) => {
		caps.value = null;
		if (!name) return;
		try {
			caps.value = await api.capabilities(name);
		} catch {
			caps.value = null;
		}
	},
	{ immediate: true },
);

function pickPrinterPaper(value: string) {
	if (!value.startsWith("cap:")) return;
	const paper = caps.value?.papers[Number(value.slice(4))];
	if (paper) {
		form.paperMode = "custom";
		form.customW = Math.round(Math.min(paper.width_mm, paper.height_mm) * 10) / 10;
		form.customH = Math.round(Math.max(paper.width_mm, paper.height_mm) * 10) / 10;
	}
	printerPaperChoice.value = "";
}

/** Warning when the custom size is beyond what the printer accepts (usually a unit mix-up). */
const sizeWarning = computed(() => {
	if (form.paperMode !== "custom") return "";
	const w = num(form.customW);
	const h = num(form.customH);
	const maxW = caps.value?.max_width_mm;
	const maxH = caps.value?.max_height_mm;
	const problems: string[] = [];
	if (w !== undefined && maxW && w > maxW + 0.5) problems.push(`max. width ${Math.round(maxW)} mm`);
	if (h !== undefined && maxH && h > maxH + 0.5) problems.push(`max. length ${Math.round(maxH)} mm`);
	if (!problems.length) return "";
	const cm = (v?: number) => (v === undefined ? "" : `${Math.round(v) / 10} cm`);
	return `Too large for this printer (${problems.join(", ")}). ${w} mm = ${cm(w)}${
		h !== undefined ? `, ${h} mm = ${cm(h)}` : ""
	} — did you mean ${w !== undefined ? w / 10 : ""}${h !== undefined ? `x${h / 10}` : ""} mm?`;
});

/** Quick fills for common receipt/label sizes: [width, height or "" for roll]. */
const QUICK_SIZES: [string, number, number | ""][] = [
	["Struk 58 mm", 58, ""],
	["Struk 80 mm", 80, ""],
	["Label 100×150", 100, 150],
	["Label 50×25", 50, 25],
];

function quickSize(w: number, h: number | "") {
	form.paperMode = "custom";
	form.customW = w;
	form.customH = h;
}

/** Vue turns `type="number"` inputs into numbers, other inputs stay strings. */
function num(v: string | number): number | undefined {
	const s = String(v).trim().replace(",", ".");
	const n = Number(s);
	return s === "" || Number.isNaN(n) ? undefined : n;
}

function toSettings(): PrintSettings {
	const s: PrintSettings = {};
	if (form.printer) s.printer = form.printer;
	if (num(form.copies) !== undefined) s.copies = num(form.copies);
	if (form.collate) s.collate = form.collate === "true";
	if (form.orientation) s.orientation = form.orientation as PrintSettings["orientation"];
	if (form.paperMode === "custom") {
		const w = num(form.customW);
		const h = num(form.customH);
		s.paper = h === undefined ? `${w}mm` : `${w}x${h}mm`;
	} else if (form.paperMode) s.paper = form.paperMode;
	if (form.scaleMode === "percent") s.scale = `${form.scalePercent}%`;
	else if (form.scaleMode) s.scale = form.scaleMode;
	if (form.align) s.align = form.align as PrintSettings["align"];
	if (form.margins.some((m) => num(m) !== undefined)) {
		s.margins = form.margins.map((m) => num(m) ?? 0).join(",");
	}
	if (num(form.offsetX) !== undefined) s.offset_x = num(form.offsetX);
	if (num(form.offsetY) !== undefined) s.offset_y = num(form.offsetY);
	if (form.pages.trim()) s.pages = form.pages.trim();
	if (form.duplex) s.duplex = form.duplex as PrintSettings["duplex"];
	if (form.color) s.color = form.color as PrintSettings["color"];
	if (form.tray) s.tray = form.tray;
	if (num(form.dpi) !== undefined) s.dpi = num(form.dpi);
	return s;
}

// ---- optional design ------------------------------------------------------
const design = ref<Template | undefined>(
	props.settings.design ? JSON.parse(JSON.stringify(props.settings.design)) : undefined,
);
const editingDesign = ref(false);
const thumb = ref("");
watch(
	design,
	async (d) => {
		try {
			thumb.value = d ? await api.previewTemplate(JSON.parse(JSON.stringify(d)), { ...d.sample }, 4) : "";
		} catch {
			thumb.value = "";
		}
	},
	{ immediate: true, deep: true },
);

function resetPaperLayoutFields() {
	form.paperMode = "";
	form.customW = "";
	form.customH = "";
	form.orientation = "";
	form.scaleMode = "";
	form.scalePercent = "100";
	form.align = "";
	form.margins.splice(0, form.margins.length, "", "", "", "");
	form.pages = "";
}

/** Starter design, sized from the preset's custom paper when there is one. */
function newDesign(): Template {
	const w = form.paperMode === "custom" ? (num(form.customW) ?? 60) : 60;
	const h = form.paperMode === "custom" ? (num(form.customH) ?? 40) : 40;
	return {
		width_mm: w,
		height_mm: h,
		elements: [
			{ type: "text", id: "t1", x: 3, y: 3, w: w - 6, h: 8, text: "Name: {{nama}}", size: 12, bold: true, align: "left", valign: "top", wrap: true, fit: true },
			{ type: "text", id: "t2", x: 3, y: 12, w: w - 6, h: 6, text: "Code: {{kode}}", size: 10, bold: false, align: "left", valign: "top", wrap: true, fit: true },
			{ type: "barcode", id: "b1", x: 3, y: 20, w: Math.min(40, w - 6), h: Math.min(15, h - 22), value: "{{kode}}", show_text: true },
		],
		sample: { nama: "Product A", kode: "SKU-001" },
	};
}

/** The design editor saves the whole preset in one go. */
function applyDesign(d: Template) {
	const isNewDesign = !design.value;
	design.value = d;
	editingDesign.value = false;
	if (isNewDesign) {
		resetPaperLayoutFields();
	}
	save();
}

async function save() {
	error.value = "";
	saving.value = true;
	try {
		const preset: Preset = { ...toSettings(), ...(design.value ? { design: design.value } : {}) };
		const config = await api.savePreset(form.name.trim(), preset, props.name);
		emit("saved", config);
	} catch (e) {
		error.value = errorText(e);
	} finally {
		saving.value = false;
	}
}
</script>

<template>
	<DesignEditor
		v-if="editingDesign"
		:preset-name="form.name"
		:template="design ?? newDesign()"
		:local-ip="localIp"
		:server-port="serverPort"
		@apply="applyDesign"
		@cancel="editingDesign = false"
	/>
	<form v-else class="stack" @submit.prevent="save">
		<h3 class="section-title">{{ name ? `Edit preset "${name}"` : "New preset" }}</h3>
		<p class="hint">Fields left as "—" inherit the default or request parameters.</p>

		<label class="field">
				<span>Preset name</span>
				<input v-model="form.name" required maxlength="64" placeholder="e.g. receipt" />
		</label>

		<fieldset>
			<legend>Label design (optional)</legend>
			<template v-if="design">
				<div class="row design-row">
					<img v-if="thumb" :src="thumb" class="design-thumb" alt="" />
					<div class="grow min0">
						<div class="strong">{{ design.width_mm }} × {{ design.height_mm }} mm</div>
						<div class="meta">Fields: {{ templateFields(design).join(", ") || "—" }}</div>
					</div>
				</div>
				<p class="hint">The web app only needs to send JSON to <code>POST /print</code> with this <code>"preset"</code>.</p>
				<div class="row wrap">
					<button type="button" class="ghost small" @click="editingDesign = true">Edit design</button>
					<button type="button" class="ghost small danger-text" @click="design = undefined">Remove design</button>
				</div>
			</template>
			<template v-else>
				<p class="hint">
					Without a design, the preset prints PDFs. With a design (a label/receipt containing fields such as
					<code>{{ "{" + "{name}" + "}" }}</code>), the web app only needs to send the data.
				</p>
				<div class="row">
					<button type="button" class="ghost small" @click="editingDesign = true">+ Create design</button>
				</div>
			</template>
		</fieldset>

		<fieldset>
			<legend>Printer</legend>
			<label class="field">
				<span>Printer</span>
				<AutocompleteField v-model="form.printer" :options="printerOptions" aria-label="Printer preset" />
			</label>
			<div class="grid2">
				<label class="field">
					<span>Jumlah copy</span>
					<input v-model="form.copies" type="number" min="1" max="999" placeholder="—" />
				</label>
				<label class="field">
					<span>Collate</span>
					<AutocompleteField v-model="form.collate" :options="collateOptions" aria-label="Collate" />
				</label>
			</div>
		</fieldset>

		<fieldset>
			<legend>Paper &amp; layout</legend>
			<p v-if="design" class="hint">
				Paper size ({{ design.width_mm }} × {{ design.height_mm }} mm), scale, margins and orientation follow the design.
			</p>
			<template v-if="!design">
			<label class="field">
					<span>Paper size</span>
					<AutocompleteField v-model="form.paperMode" :options="paperModeOptions" :limit="12" aria-label="Paper size" />
			</label>
			<template v-if="form.paperMode === 'custom'">
				<div class="grid2">
					<label class="field">
						<span>Width (mm)</span>
						<input v-model="form.customW" type="number" min="10" max="2000" step="any" required placeholder="e.g. 80" />
					</label>
					<label class="field">
						<span>Length (mm)</span>
						<input v-model="form.customH" type="number" min="10" max="5000" step="any" placeholder="blank = roll" />
					</label>
				</div>
				<p v-if="sizeWarning" class="error">⚠ {{ sizeWarning }}</p>
				<p class="hint">
					Paper uses exactly this width × length (width runs across the paper feed direction).
					A blank length means roll/receipt paper whose length follows the document content.
				</p>
				<div class="row wrap">
					<button v-for="[label, w, h] in QUICK_SIZES" :key="label" type="button" class="ghost small" @click="quickSize(w, h)">
						{{ label }}
					</button>
				</div>
			</template>
			<label v-if="caps?.papers.length" class="field">
				<span>Or use a printer size</span>
				<AutocompleteField v-model="printerPaperChoice" :options="printerPaperOptions" aria-label="Printer paper size" @update:model-value="pickPrinterPaper" />
			</label>

			<div class="grid2">
				<label class="field">
					<span>{{ form.paperMode === "custom" ? "Feed direction" : "Orientation" }}</span>
					<AutocompleteField :model-value="form.orientation" :options="orientationOptions" aria-label="Orientation" @update:model-value="form.orientation = $event" />
				</label>
				<label class="field">
					<span>Position</span>
					<AutocompleteField v-model="form.align" :options="alignOptions" aria-label="Content position" />
				</label>
			</div>

			<div class="grid2">
				<label class="field">
					<span>Scale</span>
					<AutocompleteField v-model="form.scaleMode" :options="scaleOptions" aria-label="Scale" />
				</label>
				<label v-if="form.scaleMode === 'percent'" class="field">
					<span>Percentage</span>
					<input v-model="form.scalePercent" type="number" min="1" max="1000" required />
				</label>
			</div>

			<div class="field">
				<span>Margins (mm): top · right · bottom · left</span>
				<div class="grid4">
					<input v-for="(_, i) in form.margins" :key="i" v-model="form.margins[i]" type="number" min="0" step="any" placeholder="—" />
				</div>
			</div>
			</template>
			<div class="grid2">
				<label class="field">
					<span>Offset X (mm, + right)</span>
					<input v-model="form.offsetX" type="number" step="any" placeholder="—" />
				</label>
				<label class="field">
					<span>Offset Y (mm, + down)</span>
					<input v-model="form.offsetY" type="number" step="any" placeholder="—" />
				</label>
			</div>
			<label v-if="!design" class="field">
				<span>Pages</span>
				<input v-model="form.pages" placeholder="— all (e.g. 1-3,5,8-)" />
			</label>
		</fieldset>

		<fieldset>
			<legend>Printer options</legend>
			<div class="grid2">
				<label class="field">
					<span>Duplex{{ caps && !caps.duplex ? " (possibly unsupported)" : "" }}</span>
					<AutocompleteField v-model="form.duplex" :options="duplexOptions" aria-label="Duplex" />
				</label>
				<label class="field">
					<span>Colour</span>
					<AutocompleteField v-model="form.color" :options="colorOptions" aria-label="Colour" />
				</label>
			</div>
			<div class="grid2">
				<label class="field">
					<span>Tray</span>
					<AutocompleteField v-if="caps?.trays.length" v-model="form.tray" :options="trayOptions" aria-label="Printer tray" />
					<input v-else v-model="form.tray" placeholder="—" />
				</label>
				<label class="field">
					<span>DPI (Windows)</span>
					<input v-model="form.dpi" type="number" min="72" max="600" placeholder="— automatic" />
				</label>
			</div>
		</fieldset>

		<p v-if="error" class="error">{{ error }}</p>
		<div class="row end">
			<button type="button" class="ghost" @click="emit('cancel')">Cancel</button>
			<button type="submit" class="primary" :disabled="saving">{{ saving ? "Saving…" : "Save" }}</button>
		</div>
	</form>
</template>

<style scoped>
.design-row {
	align-items: center;
}
.design-thumb {
	max-width: 96px;
	max-height: 72px;
	border: 1px solid var(--line);
	border-radius: 4px;
	background: #fff;
}
.danger-text {
	color: var(--danger) !important;
}
</style>
