<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { api, type Template } from "@/shared/api";
import { templateAutoFields, templateFields } from "@/shared/lib/template";
import DesignCanvas from "./DesignCanvas.vue";
import DesignLayersPanel from "./DesignLayersPanel.vue";
import DesignPreviewPanel from "./DesignPreviewPanel.vue";
import DesignPropertiesPanel from "./DesignPropertiesPanel.vue";
import DesignToolbar from "./DesignToolbar.vue";
import { useDesignClipboard } from "./useDesignClipboard";
import { useDesignImages } from "./useDesignImages";
import { useDesignLayers } from "./useDesignLayers";
import { useDesignPointer } from "./useDesignPointer";

const props = defineProps<{
	/** Name of the preset this design belongs to (for the example request). */
	presetName: string;
	template: Template;
	localIp: string;
	serverPort: number;
}>();
const emit = defineEmits<{
	/** The edited design; the preset form saves it together with the preset. */
	(e: "apply", design: Template): void;
	(e: "cancel"): void;
}>();

const footerOptions = [
	{ value: "last_page", label: "At the end only (totals, signature)" },
	{ value: "every_page", label: "Every page" },
];
const QUICK_SIZES: [string, number, number][] = [
	["58 × 40", 58, 40], ["80 × 50", 80, 50], ["60 × 75", 60, 75],
	["100 × 50", 100, 50], ["100 × 150", 100, 150],
];
const t = reactive<Template>(JSON.parse(JSON.stringify(props.template)));
if (!t.layout) t.layout = { mode: "label" };
const list = computed(() => (t.layout?.mode === "list" ? t.layout : null));
const canvasH = computed(() => list.value ? list.value.header_mm + list.value.row_mm + list.value.footer_mm : t.height_mm);
const holderWidth = ref(360);
const scale = computed(() => Math.max(1, Math.min(holderWidth.value / t.width_mm, 560 / canvasH.value, 14)));
const preview = ref("");
const previewError = ref("");
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
			} catch (error) {
				if (seq === previewSeq) previewError.value = error instanceof Error ? error.message : String(error);
			}
		}, 150);
	},
	{ immediate: true },
);

const fields = computed(() => templateFields(t));
function ensureSample(field: string) {
	if (!(field in t.sample) || !t.sample[field]) t.sample[field] = "Contoh";
}
const autoFields = computed(() => templateAutoFields(t));
const AUTO_HINT: Record<string, string> = {
	no: "item number (1, 2, 3…)",
	halaman: "page number",
	total_halaman: "total pages",
	jumlah_data: "item count",
};
watch(fields, (currentFields) => {
	for (const field of currentFields) if (!(field in t.sample)) t.sample[field] = "Contoh";
}, { immediate: true });

const { selected, current, layerRows, setMode, bandOf, add, removeSelected, selectAndReorder } =
	useDesignLayers(t, list, canvasH, ensureSample);
const pointer = useDesignPointer(t.elements, selected, scale);
const { copiedElement, copySelected, pasteCopied, duplicateSelected } = useDesignClipboard(t, selected, pointer.moveBy);
const { fileInput, imageError, pickImage, onImageFile } = useDesignImages(t, canvasH, selected);

function onKey(event: KeyboardEvent) {
	const target = event.target as HTMLElement;
	if (target.closest("input, textarea, select, [contenteditable=true]")) return;
	const modifier = event.metaKey || event.ctrlKey;
	if (modifier && event.key.toLowerCase() === "c" && selected.value !== null) {
		event.preventDefault();
		copySelected();
		return;
	}
	if (modifier && event.key.toLowerCase() === "v" && copiedElement.value) {
		event.preventDefault();
		pasteCopied();
		return;
	}
	if (selected.value === null || !target.closest(".stage")) return;
	const step = event.shiftKey ? 5 : 0.5;
	const moves: Record<string, [number, number]> = {
		ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step],
	};
	if (moves[event.key]) {
		event.preventDefault();
		pointer.moveBy(t.elements[selected.value], ...moves[event.key]);
	} else if (event.key === "Delete" || event.key === "Backspace") {
		event.preventDefault();
		removeSelected();
	}
}

function insertField(field: string) {
	const element = current.value;
	if (!element) return;
	if (element.type === "text") {
		element.text += `{{${field}}}`;
		ensureSample(field);
	} else if (element.type === "barcode" || element.type === "qr") {
		element.value = `{{${field}}}`;
		ensureSample(field);
	}
}
const bandFields = computed(() => {
	const shared = new Set<string>();
	const row = new Set<string>();
	for (const element of t.elements) {
		const one: Template = { width_mm: 1, height_mm: 1, elements: [element], sample: {} };
		for (const field of templateFields(one)) (list.value && bandOf(element) !== "row" ? shared : row).add(field);
	}
	return { shared: [...shared].sort(), row: [...row].sort() };
});
const exampleBody = computed(() => {
	const pick = (names: string[]) => Object.fromEntries(names.map((field) => [field, t.sample[field] || "…"]));
	const body: Record<string, unknown> = { preset: props.presetName || "nama-preset" };
	if (bandFields.value.shared.length) body.header = pick(bandFields.value.shared);
	const one = pick(bandFields.value.row);
	body.data = list.value ? [one, one] : [one];
	return JSON.stringify(body, null, 2);
});
function save() {
	const payload: Template = JSON.parse(JSON.stringify(t));
	payload.sample = Object.fromEntries(fields.value.map((field) => [field, payload.sample[field] ?? ""]));
	for (const field of autoFields.value) if (t.sample[field]) payload.sample[field] = t.sample[field];
	emit("apply", payload);
}
import "./design-editor.css";
</script>

<template>
	<form class="stack design-editor" @submit.prevent="save" @keydown="onKey">
		<h3 class="section-title">Design{{ presetName ? ` preset "${presetName}"` : "" }}</h3>
		<p class="hint">The preset paper size follows the design. Printer, X/Y offsets, copies and other settings come from the preset.</p>
		<DesignToolbar
			:t="t" :list="list" :quick-sizes="QUICK_SIZES" :footer-options="footerOptions" :image-error="imageError"
			@set-mode="setMode" @add="add" @choose-image="pickImage(null)"
		/>
		<input ref="fileInput" type="file" accept="image/*,.svg,.heic,.heif,.avif,.tif,.tiff,.ico" hidden @change="onImageFile" />
		<DesignCanvas
			:elements="t.elements" :list="list" :width-mm="t.width_mm" :height-mm="canvasH" :scale="scale"
			:preview="preview" :selected="selected" :bbox="pointer.bbox" :start-drag="pointer.startDrag"
			@width="holderWidth = $event" @clear-selection="selected = null"
		/>
		<p v-if="previewError" class="error">⚠ {{ previewError }}</p>
		<p class="hint">Move elements with the mouse or arrow keys (Shift = 5 mm). The preview is the actual print output.</p>
		<DesignLayersPanel
			:rows="layerRows" :selected="selected" :element-count="t.elements.length" :has-copied="!!copiedElement"
			@copy="copySelected" @paste="pasteCopied" @select="selected = $event" @reorder="selectAndReorder"
		/>
		<DesignPropertiesPanel
			:current="current" :selected="selected" :fields="fields"
			@insert-field="insertField" @replace-image="pickImage" @duplicate="duplicateSelected" @remove="removeSelected"
		/>
		<DesignPreviewPanel
			:template="t" :list="list" :fields="fields" :auto-fields="autoFields" :auto-hint="AUTO_HINT"
			:example-body="exampleBody" :local-ip="localIp" :server-port="serverPort"
		/>
		<div class="row end">
			<button type="button" class="ghost" @click="emit('cancel')">Cancel</button>
			<button type="submit" class="primary">Save design &amp; preset</button>
		</div>
	</form>
</template>
