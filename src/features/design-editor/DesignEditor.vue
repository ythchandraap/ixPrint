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
import { useDesignHistory } from "./useDesignHistory";

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
  ["58 × 40", 58, 40],
  ["80 × 50", 80, 50],
  ["60 × 75", 60, 75],
  ["100 × 50", 100, 50],
  ["100 × 150", 100, 150],
];
const t = reactive<Template>(JSON.parse(JSON.stringify(props.template)));
if (!t.layout) t.layout = { mode: "label" };
const list = computed(() => (t.layout?.mode === "list" ? t.layout : null));
const canvasH = computed(() =>
  list.value ? list.value.header_mm + list.value.row_mm + list.value.footer_mm : t.height_mm,
);
const holderWidth = ref(360);
const zoom = ref(100);
const showGrid = ref(true);
const snapEnabled = ref(true);
const fitScale = computed(() => Math.max(1, Math.min((holderWidth.value - 28) / t.width_mm, 560 / canvasH.value, 14)));
const scale = computed(() => fitScale.value * zoom.value / 100);
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
watch(
  fields,
  (currentFields) => {
    for (const field of currentFields) if (!(field in t.sample)) t.sample[field] = "Contoh";
    t.required_fields = (t.required_fields ?? []).filter((field) => currentFields.includes(field));
  },
  { immediate: true },
);

const { selected, current, layerRows, setMode, bandOf, add, removeSelected, selectAndReorder } = useDesignLayers(
  t,
  list,
  canvasH,
  ensureSample,
);
const pointer = useDesignPointer(t.elements, selected, scale, snapEnabled);
const clippedElements = computed(() => t.elements.flatMap((element, index) => {
  const box = pointer.bbox(element);
  return box.x < 0 || box.y < 0 || box.x + box.w > t.width_mm || box.y + box.h > canvasH.value
    ? [index + 1] : [];
}));
const history = useDesignHistory(t);
watch(() => t.elements.length, (length) => {
  if (selected.value !== null && selected.value >= length) selected.value = null;
});
const { copiedElement, copySelected, pasteCopied, duplicateSelected } = useDesignClipboard(t, selected, pointer.moveBy);
const { fileInput, imageError, pickImage, onImageFile } = useDesignImages(t, canvasH, selected);

function onKey(event: KeyboardEvent) {
  const target = event.target as HTMLElement;
  if (target.closest("input, textarea, select, [contenteditable=true]")) return;
  const modifier = event.metaKey || event.ctrlKey;
  if (modifier && event.key.toLowerCase() === "z") {
    event.preventDefault();
    if (event.shiftKey) history.redo();
    else history.undo();
    return;
  }
  if (modifier && event.key.toLowerCase() === "y") {
    event.preventDefault();
    history.redo();
    return;
  }
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
  if (selected.value === null || modifier || event.altKey) return;
  const step = event.shiftKey ? 5 : 0.5;
  const moves: Record<string, [number, number]> = {
    ArrowLeft: [-step, 0],
    ArrowRight: [step, 0],
    ArrowUp: [0, -step],
    ArrowDown: [0, step],
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
function alignSelected(edge: "left" | "hcenter" | "right" | "top" | "vcenter" | "bottom") {
  const element = current.value;
  if (!element) return;
  const box = pointer.bbox(element);
  const dx = edge === "left" ? -box.x : edge === "hcenter" ? (t.width_mm - box.w) / 2 - box.x : edge === "right" ? t.width_mm - box.w - box.x : 0;
  const dy = edge === "top" ? -box.y : edge === "vcenter" ? (canvasH.value - box.h) / 2 - box.y : edge === "bottom" ? canvasH.value - box.h - box.y : 0;
  pointer.moveBy(element, dx, dy);
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
    <p class="hint">
      The preset paper size follows the design. Printer, X/Y offsets, copies and other settings come from the preset.
    </p>
    <DesignToolbar
      :t="t"
      :list="list"
      :quick-sizes="QUICK_SIZES"
      :footer-options="footerOptions"
      :image-error="imageError"
      @set-mode="setMode"
      @add="add"
      @choose-image="pickImage(null)"
    />
    <div class="row wrap design-view-tools">
      <button type="button" class="ghost small" :disabled="!history.canUndo.value" @click="history.undo()">Undo</button>
      <button type="button" class="ghost small" :disabled="!history.canRedo.value" @click="history.redo()">Redo</button>
      <span class="grow"></span>
      <label class="row tight check"><input v-model="showGrid" type="checkbox" /> 5 mm grid</label>
      <label class="row tight check"><input v-model="snapEnabled" type="checkbox" /> Snap 0.5 mm</label>
      <label class="row tight zoom-control">Zoom
        <select v-model.number="zoom" aria-label="Canvas zoom">
          <option v-for="level in [50, 75, 100, 125, 150, 200]" :key="level" :value="level">{{ level }}%</option>
        </select>
      </label>
    </div>
    <input
      ref="fileInput"
      type="file"
      accept="image/*,.svg,.heic,.heif,.avif,.tif,.tiff,.ico"
      hidden
      @change="onImageFile"
    />
    <DesignCanvas
      :elements="t.elements"
      :list="list"
      :width-mm="t.width_mm"
      :height-mm="canvasH"
      :scale="scale"
      :preview="preview"
      :show-grid="showGrid"
      :selected="selected"
      :bbox="pointer.bbox"
      :start-drag="pointer.startDrag"
      @width="holderWidth = $event"
      @clear-selection="selected = null"
    />
    <p v-if="previewError" class="error">⚠ {{ previewError }}</p>
    <p v-if="clippedElements.length" class="error">⚠ Elements {{ clippedElements.join(", ") }} extend beyond the design edge and may be clipped when printed.</p>
    <p class="hint">
      Select an element, then use arrow keys to move it 0.5 mm (Shift = 5 mm). Rulers show distances in mm.
      The preview is the actual print output.
    </p>
    <DesignLayersPanel
      :rows="layerRows"
      :selected="selected"
      :element-count="t.elements.length"
      :has-copied="!!copiedElement"
      @copy="copySelected"
      @paste="pasteCopied"
      @select="selected = $event"
      @reorder="selectAndReorder"
    />
    <DesignPropertiesPanel
      :current="current"
      :selected="selected"
      :fields="fields"
      @align="alignSelected"
      @insert-field="insertField"
      @replace-image="pickImage"
      @duplicate="duplicateSelected"
      @remove="removeSelected"
    />
    <DesignPreviewPanel
      :template="t"
      :list="list"
      :fields="fields"
      :auto-fields="autoFields"
      :auto-hint="AUTO_HINT"
      :example-body="exampleBody"
      :local-ip="localIp"
      :server-port="serverPort"
    />
    <div class="row end">
      <button type="button" class="ghost" @click="emit('cancel')">Cancel</button>
      <button type="submit" class="primary">Save design &amp; preset</button>
    </div>
  </form>
</template>
