<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { ArrowLeft, Check, CircleDot, X } from "lucide-vue-next";
import { api, type Template } from "@/shared/api";
import { templateAutoFields, templateFields } from "@/shared/lib/template";
import DesignCanvas from "./DesignCanvas.vue";
import DesignApiPanel from "./DesignApiPanel.vue";
import DesignLayersPanel from "./DesignLayersPanel.vue";
import DesignPagePanel from "./DesignPagePanel.vue";
import DesignPreviewPanel from "./DesignPreviewPanel.vue";
import DesignPropertiesPanel from "./DesignPropertiesPanel.vue";
import DesignToolbar from "./DesignToolbar.vue";
import { useDesignClipboard } from "./useDesignClipboard";
import { useDesignImages } from "./useDesignImages";
import { useDesignLayers } from "./useDesignLayers";
import { useDesignPointer } from "./useDesignPointer";
import { useDesignHistory } from "./useDesignHistory";
import { descendantElementIds, elementBox, groupBox, isElementInsideFocus, validateGroups } from "./groupModel";
import type { DesignNode } from "./useDesignLayers";

const props = defineProps<{
  /** Name of the preset this design belongs to (for the example request). */
  presetName: string;
  template: Template;
  localIp: string;
  serverPort: number;
  saveDesign: (design: Template) => Promise<boolean>;
  saving: boolean;
  saveError: string;
  leaveRequestToken: number;
}>();
const emit = defineEmits<{
  (e: "cancel"): void;
  (e: "dirty-change", dirty: boolean): void;
  (e: "leave-decision", decision: "save" | "discard" | "cancel"): void;
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
if (!t.groups) t.groups = [];
for (const field of templateFields(t)) if (!(field in t.sample)) t.sample[field] = "Contoh";
t.required_fields = (t.required_fields ?? []).filter((field) => templateFields(t).includes(field));
const savedDesign = ref(JSON.stringify(t));
const list = computed(() => (t.layout?.mode === "list" ? t.layout : null));
const canvasH = computed(() =>
  list.value ? list.value.header_mm + list.value.row_mm + list.value.footer_mm : t.height_mm,
);
const root = ref<HTMLElement | null>(null);
const inspector = ref<HTMLElement | null>(null);
const previewPanel = ref<InstanceType<typeof DesignPreviewPanel> | null>(null);
const leaveDialog = ref<HTMLDialogElement | null>(null);
const helpDialog = ref<HTMLDialogElement | null>(null);
const workspaceWidth = ref(0);
const viewportSize = ref({ width: 360, height: 420 });
const mainTab = ref<"design" | "data" | "api">("design");
const inspectorTab = ref<"object" | "layers" | "page">("object");
const inspectorOpen = ref(true);
const zoom = ref("fit");
const showGrid = ref(true);
const showRulers = ref(true);
const showBands = ref(true);
const showObjectBounds = ref(false);
const snapEnabled = ref(true);
const desktopWidth = computed(() => workspaceWidth.value >= 800);
const fitScale = computed(() => Math.max(0.1, Math.min(
  (viewportSize.value.width - (showRulers.value ? 28 : 0) - (list.value ? 54 : 0)) / t.width_mm,
  (viewportSize.value.height - (showRulers.value ? 28 : 0)) / canvasH.value,
)));
const scale = computed(() => zoom.value === "fit" ? fitScale.value : Number(zoom.value) * 96 / 25.4);
const dirty = computed(() => JSON.stringify(t) !== savedDesign.value);
watch(dirty, (value) => emit("dirty-change", value), { immediate: true });
watch(desktopWidth, (wide) => { inspectorOpen.value = wide; }, { immediate: true });
let workspaceObserver: ResizeObserver | undefined;
onMounted(() => {
  if (root.value) {
    workspaceObserver = new ResizeObserver(([entry]) => { workspaceWidth.value = entry.contentRect.width; });
    workspaceObserver.observe(root.value);
  }
});
onBeforeUnmount(() => workspaceObserver?.disconnect());
const leaveDialogOpen = ref(false);
const helpDialogOpen = ref(false);
const leavingForApp = ref(false);
watch(leaveDialogOpen, async (open) => {
  await nextTick();
  if (open && leaveDialog.value && !leaveDialog.value.open) {
    leaveDialog.value.showModal();
    leaveDialog.value.querySelector<HTMLButtonElement>("button")?.focus();
  } else if (!open && leaveDialog.value?.open) leaveDialog.value.close();
});
watch(helpDialogOpen, async (open) => {
  await nextTick();
  if (open && helpDialog.value && !helpDialog.value.open) {
    helpDialog.value.showModal();
    helpDialog.value.querySelector<HTMLButtonElement>("button")?.focus();
  } else if (!open && helpDialog.value?.open) helpDialog.value.close();
});
watch(() => props.leaveRequestToken, (token, previous) => {
  if (token !== previous && token > 0) {
    leavingForApp.value = true;
    leaveDialogOpen.value = true;
  }
});
function openLeaveDialog(appNavigation = false) {
  leavingForApp.value = appNavigation;
  leaveDialogOpen.value = true;
}
function cancelLeave() {
  leaveDialogOpen.value = false;
  if (leavingForApp.value) emit("leave-decision", "cancel");
  leavingForApp.value = false;
}
function discardAndLeave() {
  leaveDialogOpen.value = false;
  if (leavingForApp.value) emit("leave-decision", "discard");
  else emit("cancel");
  leavingForApp.value = false;
}
function requestBack() {
  if (dirty.value) openLeaveDialog();
  else emit("cancel");
}
async function saveAndLeave() {
  const succeeded = await save();
  if (succeeded) leaveDialogOpen.value = false;
}
const pageLabel = computed(() => t.layout?.mode === "list"
  ? t.layout.roll ? `List · ${t.width_mm} mm roll` : `List · ${t.width_mm} × ${t.height_mm} mm`
  : `Label · ${t.width_mm} × ${t.height_mm} mm`);
const groupSummary = computed(() => {
  const node = selected.value.length === 1 && selected.value[0].kind === "group" ? selected.value[0] : null;
  if (!node) return null;
  const group = t.groups?.find((item) => item.id === node.id);
  return group ? { name: group.name, count: descendantElementIds(t, group.id).size } : null;
});
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
onBeforeUnmount(() => clearTimeout(previewTimer));

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

const layers = useDesignLayers(
  t,
  list,
  canvasH,
  ensureSample,
);
const { selected, selectedIndex, selectedElementId, current, layerRows, structureRows, groupPathRows, focusGroupId,
  moveTargets, setMode, bandOf, add, removeSelected, selectAndReorder, selectNode, selectFromStructure, resolveElement,
  enterGroup, setFocus, exitFocus, clearSelection, createGroup, ungroupSelection, moveSelectionTo, renameGroup,
  setMarkSelection, restoreEditorState } = layers;
const groupError = ref(validateGroups(t));
const geometryState = () => JSON.stringify({ width_mm: t.width_mm, height_mm: t.height_mm, layout: t.layout });
let previousGeometry = geometryState();
watch(geometryState, (value) => {
  if (value === previousGeometry) { groupError.value = validateGroups(t); return; }
  const error = validateGroups(t);
  if (error && t.groups?.length) {
    const previous = JSON.parse(previousGeometry) as Pick<Template, "width_mm" | "height_mm" | "layout">;
    t.width_mm = previous.width_mm;
    t.height_mm = previous.height_mm;
    t.layout = previous.layout;
    groupError.value = error;
    return;
  }
  previousGeometry = value;
  groupError.value = error;
}, { flush: "sync" });
watch(() => JSON.stringify({
  groups: t.groups,
  positions: t.elements.map((element) => element.type === "line"
    ? [element.id, element.x1, element.y1, element.x2, element.y2]
    : [element.id, element.x, element.y]),
}), () => { groupError.value = validateGroups(t); }, { flush: "post" });
const history = useDesignHistory(t);
history.bindView(
  () => ({ selected: selected.value, focusGroupId: focusGroupId.value }),
  (state) => restoreEditorState((state ?? {}) as { selected?: DesignNode[]; focusGroupId?: string | null }),
);
setMarkSelection(history.markView);
const nodeElementIds = (node: DesignNode) => node.kind === "element"
  ? new Set([node.id]) : descendantElementIds(t, node.id);
function constrainMove(ids: string[], dx: number, dy: number): [number, number] {
  const boxes = t.elements.filter((element) => ids.includes(element.id)).map((element) => ({ element, box: elementBox(element) }));
  if (!boxes.length) return [0, 0];
  const minX = Math.max(...boxes.map(({ box }) => -box.x));
  const maxX = Math.min(...boxes.map(({ box }) => t.width_mm - box.x - box.w));
  let minY = Math.max(...boxes.map(({ box }) => -box.y));
  let maxY = Math.min(...boxes.map(({ box }) => canvasH.value - box.y - box.h));
  if (list.value) {
    for (const { element, box } of boxes) {
      const band = bandOf(element);
      const start = band === "header" ? 0 : band === "row" ? list.value.header_mm : list.value.header_mm + list.value.row_mm;
      const end = band === "header" ? list.value.header_mm : band === "row" ? list.value.header_mm + list.value.row_mm : canvasH.value;
      minY = Math.max(minY, start - box.y);
      maxY = Math.min(maxY, end - box.y - box.h);
    }
  }
  if (minX > maxX || minY > maxY) return [0, 0];
  return [Math.max(minX, Math.min(maxX, dx)), Math.max(minY, Math.min(maxY, dy))];
}
const pointer = useDesignPointer(t, selected, selectedIndex, scale, snapEnabled, nodeElementIds, resolveElement,
  selectNode, constrainMove, history);
const clippedElements = computed(() => t.elements.flatMap((element, index) => {
  const box = pointer.bbox(element);
  return box.x < 0 || box.y < 0 || box.x + box.w > t.width_mm || box.y + box.h > canvasH.value
    ? [index + 1] : [];
}));
const { copied, copySelected, pasteCopied, duplicateSelected } = useDesignClipboard(
  t, selected, focusGroupId, pointer.moveSelectionBy, history.markView,
);
const { fileInput, imageError, pickImage, onImageFile } = useDesignImages(t, canvasH, (element) => {
  if (focusGroupId.value) {
    const existingIds = descendantElementIds(t, focusGroupId.value);
    if (list.value && element.type === "image") {
      const band = bandOf(t.elements.find((item) => existingIds.has(item.id)) ?? element);
      const top = band === "header" ? 0 : band === "row" ? list.value.header_mm : list.value.header_mm + list.value.row_mm;
      const bottom = band === "header" ? list.value.header_mm : band === "row" ? list.value.header_mm + list.value.row_mm : canvasH.value;
      element.y = top;
      element.h = Math.min(element.h, Math.max(1, bottom - top));
    }
    t.groups?.find((group) => group.id === focusGroupId.value)?.elementIds.push(element.id);
  }
  selected.value = [{ kind: "element", id: element.id }];
  history.markView();
});
const selectedGroupBoxes = computed(() => selected.value.filter((node) => node.kind === "group")
  .flatMap((node) => { const box = groupBox(t, node.id); return box ? [{ id: node.id, box }] : []; }));
const selectedElementIds = computed(() => selected.value.filter((node) => node.kind === "element").map((node) => node.id));
const canGroup = computed(() => selected.value.length >= 2);
const canUngroup = computed(() => selected.value.length === 1 && selected.value[0].kind === "group");
const groupDeleteCount = computed(() => new Set(selected.value.flatMap((node) => [...nodeElementIds(node)])).size);

function handleDoubleClick(elementId: string) {
  const node = resolveElement(elementId);
  if (node?.kind === "group") enterGroup(node.id);
}
function handleGroup() {
  const error = createGroup();
  groupError.value = error ?? validateGroups(t);
}
function handleUngroup() {
  const error = ungroupSelection();
  groupError.value = error ?? validateGroups(t);
}
function handleMove(targetId: string | null) {
  const error = moveSelectionTo(targetId);
  groupError.value = error ?? validateGroups(t);
}
function handleRename(id: string, name: string) {
  groupError.value = renameGroup(id, name) ?? validateGroups(t);
}
function handleDelete() {
  if (selected.value.some((node) => node.kind === "group") && !window.confirm(`Delete selected group(s) and ${groupDeleteCount.value} object(s)?`)) return;
  removeSelected();
  groupError.value = validateGroups(t);
}
function handleMode(mode: "label" | "list") {
  const error = setMode(mode);
  groupError.value = error ?? validateGroups(t);
}
function handleAdd(type: Parameters<typeof add>[0] | "image") {
  if (type === "image") { pickImage(null); return; }
  const error = add(type);
  groupError.value = error ?? validateGroups(t);
}
function insideCurrentFocus(id: string) { return isElementInsideFocus(t, id, focusGroupId.value); }
function openInspector(tab: "object" | "layers" | "page" = inspectorTab.value) {
  inspectorTab.value = tab;
  inspectorOpen.value = true;
}
function handleCanvasSelection(id: string) {
  const node = resolveElement(id);
  if (!node) return;
  inspectorTab.value = "object";
  if (desktopWidth.value) inspectorOpen.value = true;
}
function closeInspector() {
  inspectorOpen.value = false;
  void nextTick(() => root.value?.querySelector<HTMLButtonElement>(".inspector-toggle")?.focus());
}
function showPreview() {
  mainTab.value = "data";
  void nextTick(() => previewPanel.value?.previewCurrent());
}

function onKey(event: KeyboardEvent) {
  if (event.defaultPrevented) return;
  const target = event.target as HTMLElement;
  if (target.closest("input, textarea, select, [contenteditable], dialog, [role=dialog]")) return;
  const modifier = event.metaKey || event.ctrlKey;
  if (event.key === "Escape") {
    event.preventDefault();
    if (!desktopWidth.value && inspectorOpen.value && inspector.value?.contains(target)) closeInspector();
    else exitFocus();
    return;
  }
  if (event.key === "?") { event.preventDefault(); helpDialogOpen.value = true; return; }
  if (modifier && event.key.toLowerCase() === "g") {
    if (event.shiftKey && canUngroup.value) { event.preventDefault(); handleUngroup(); }
    else if (!event.shiftKey && canGroup.value) { event.preventDefault(); handleGroup(); }
    return;
  }
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
  if (modifier && event.key.toLowerCase() === "c" && selected.value.length) {
    event.preventDefault();
    copySelected();
    return;
  }
  if (modifier && event.key.toLowerCase() === "v" && copied.value) {
    event.preventDefault();
    pasteCopied();
    return;
  }
  if (!selected.value.length || modifier || event.altKey) return;
  const step = event.shiftKey ? 5 : 0.5;
  const moves: Record<string, [number, number]> = {
    ArrowLeft: [-step, 0],
    ArrowRight: [step, 0],
    ArrowUp: [0, -step],
    ArrowDown: [0, step],
  };
  if (moves[event.key]) {
    event.preventDefault();
    history.commit();
    pointer.moveSelectionBy(...moves[event.key]);
    history.commit();
  } else if (event.key === "Delete" || event.key === "Backspace") {
    event.preventDefault();
    handleDelete();
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
    const one: Template = { width_mm: 1, height_mm: 1, elements: [element], groups: [], sample: {} };
    for (const field of templateFields(one)) (list.value && bandOf(element) !== "row" ? shared : row).add(field);
  }
  return { shared: [...shared].sort(), row: [...row].sort() };
});
const exampleBody = computed(() => {
  const pick = (names: string[]) => Object.fromEntries(names.map((field) => [field, t.sample[field] || "…"]));
  const body: Record<string, unknown> = { preset: props.presetName };
  if (bandFields.value.shared.length) body.header = pick(bandFields.value.shared);
  const one = pick(bandFields.value.row);
  body.data = list.value ? [one, one] : [one];
  return JSON.stringify(body, null, 2);
});
async function save(): Promise<boolean> {
  groupError.value = validateGroups(t);
  if (groupError.value) return false;
  const form = root.value;
  const invalid = form?.querySelector<HTMLElement>(":invalid");
  if (invalid) {
    mainTab.value = "design";
    const panel = invalid.closest<HTMLElement>("[data-inspector-tab]")?.dataset.inspectorTab;
    if (panel === "object" || panel === "layers" || panel === "page") inspectorTab.value = panel;
    inspectorOpen.value = true;
    await nextTick();
    (invalid as HTMLInputElement).reportValidity?.();
    invalid.focus();
    return false;
  }
  const savedSnapshot = JSON.stringify(t);
  const payload: Template = JSON.parse(savedSnapshot);
  payload.sample = Object.fromEntries(fields.value.map((field) => [field, payload.sample[field] ?? ""]));
  for (const field of autoFields.value) if (t.sample[field]) payload.sample[field] = t.sample[field];
  const succeeded = await props.saveDesign(payload);
  if (succeeded) savedDesign.value = savedSnapshot;
  if (succeeded && leavingForApp.value) emit("leave-decision", "save");
  if (succeeded) leavingForApp.value = false;
  return succeeded;
}
import "./design-editor.css";
</script>

<template>
  <div ref="root" class="design-editor-workspace" @keydown="onKey">
    <header class="editor-header">
      <div class="editor-header-main">
        <button type="button" class="icon editor-back" aria-label="Back to preset" title="Back to preset" @click="requestBack"><ArrowLeft :size="18" /></button>
        <div class="editor-preset-heading">
          <strong class="editor-preset-name" tabindex="0" :title="presetName || 'New design'">{{ presetName || "New design" }}</strong>
          <span :class="['save-state-indicator', { dirty }]" role="img" :aria-label="dirty ? 'Unsaved changes' : 'All changes saved'" :title="dirty ? 'Unsaved changes' : 'All changes saved'">
            <CircleDot v-if="dirty" :size="16" aria-hidden="true" />
            <Check v-else :size="12" aria-hidden="true" />
          </span>
        </div>
        <span class="editor-header-spacer"></span>
        <div class="editor-header-actions">
          <button type="button" class="primary small editor-save" :disabled="saving || !!groupError" aria-describedby="save-description" @click="save">{{ saving ? "Saving…" : "Save" }}</button>
          <button type="button" class="ghost small header-preview" @click="showPreview">Preview</button>
          <span id="save-description" class="sr-only">Save the design and preset.</span>
        </div>
      </div>
      <div v-if="saveError" class="editor-header-status">
        <p v-if="saveError" class="error header-error" role="alert">{{ saveError }}</p>
      </div>
    </header>

    <nav class="editor-main-tabs" role="tablist" aria-label="Editor workspace">
      <button type="button" role="tab" id="editor-tab-design" aria-controls="editor-panel-design" :aria-selected="mainTab === 'design'" :class="{ on: mainTab === 'design' }" @click="mainTab = 'design'">Design</button>
      <button type="button" role="tab" id="editor-tab-data" aria-controls="editor-panel-data" :aria-selected="mainTab === 'data'" :class="{ on: mainTab === 'data' }" @click="mainTab = 'data'">Data &amp; Preview</button>
      <button type="button" role="tab" id="editor-tab-api" aria-controls="editor-panel-api" :aria-selected="mainTab === 'api'" :class="{ on: mainTab === 'api' }" @click="mainTab = 'api'">API</button>
    </nav>

    <section v-show="mainTab === 'design'" id="editor-panel-design" class="editor-main-panel design-main-panel" role="tabpanel" aria-labelledby="editor-tab-design">
      <DesignToolbar
        :page-label="pageLabel"
        :list="!!list"
        :can-undo="history.canUndo.value"
        :can-redo="history.canRedo.value"
        :show-grid="showGrid"
        :show-rulers="showRulers"
        :show-bands="showBands"
        :show-object-bounds="showObjectBounds"
        :snap-enabled="snapEnabled"
        :zoom="zoom"
        :inspector-open="inspectorOpen"
        @add="handleAdd"
        @undo="history.undo()"
        @redo="history.redo()"
        @open-page="openInspector('page')"
        @update:show-grid="showGrid = $event"
        @update:show-rulers="showRulers = $event"
        @update:show-bands="showBands = $event"
        @update:show-object-bounds="showObjectBounds = $event"
        @update:snap-enabled="snapEnabled = $event"
        @update:zoom="zoom = $event"
        @toggle-inspector="inspectorOpen ? closeInspector() : openInspector()"
        @help="helpDialogOpen = true"
      />
      <input ref="fileInput" type="file" accept="image/*,.svg,.heic,.heif,.avif,.tif,.tiff,.ico" hidden @change="onImageFile" />
      <nav class="design-breadcrumb" aria-label="Group path">
        <button type="button" class="breadcrumb-segment" :aria-current="focusGroupId === null ? 'page' : undefined" @click="setFocus(null)">Canvas</button>
        <template v-for="group in groupPathRows" :key="group.id">
          <span aria-hidden="true">/</span>
          <button type="button" class="breadcrumb-segment" :aria-current="focusGroupId === group.id ? 'page' : undefined" :title="group.name" @click="setFocus(group.id)">{{ group.name }}</button>
        </template>
      </nav>

      <div class="design-canvas-layout" :class="{ 'inspector-is-open': inspectorOpen }">
        <div class="canvas-column">
          <DesignCanvas
            :elements="t.elements"
            :list="list"
            :width-mm="t.width_mm"
            :height-mm="canvasH"
            :scale="scale"
            :preview="preview"
            :show-grid="showGrid"
            :show-rulers="showRulers"
            :show-bands="showBands"
            :show-object-bounds="showObjectBounds"
            :selected="selectedIndex < 0 ? null : selectedIndex"
            :selected-element-ids="selectedElementIds"
            :selected-groups="selectedGroupBoxes"
            :is-element-inside-focus="insideCurrentFocus"
            :on-double-click-element="handleDoubleClick"
            :bbox="pointer.bbox"
            :start-drag="pointer.startDrag"
            @viewport-size="viewportSize = $event"
            @select-from-canvas="handleCanvasSelection"
            @clear-selection="clearSelection"
          />
          <div class="canvas-status" aria-live="polite">
            <span v-if="previewError" class="error">{{ previewError }}</span>
            <span v-if="clippedElements.length" class="warning-status">{{ clippedElements.length }} object{{ clippedElements.length === 1 ? '' : 's' }} may be clipped when printed.</span>
            <span v-if="groupError" class="error" role="alert">{{ groupError }}</span>
            <span v-if="imageError" class="error" role="alert">{{ imageError }}</span>
            <span v-if="!previewError && !clippedElements.length && !groupError && !imageError" class="meta">Double-click to enter a group. Esc to go back.</span>
          </div>
        </div>

        <aside v-show="inspectorOpen" ref="inspector" class="design-inspector" aria-label="Inspector">
          <header class="inspector-header">
            <div class="inspector-tabs" role="tablist" aria-label="Inspector sections">
              <button type="button" role="tab" id="inspector-tab-object" aria-controls="inspector-panel-object" :aria-selected="inspectorTab === 'object'" :class="{ on: inspectorTab === 'object' }" @click="inspectorTab = 'object'">Object</button>
              <button type="button" role="tab" id="inspector-tab-layers" aria-controls="inspector-panel-layers" :aria-selected="inspectorTab === 'layers'" :class="{ on: inspectorTab === 'layers' }" @click="inspectorTab = 'layers'">Layers</button>
              <button type="button" role="tab" id="inspector-tab-page" aria-controls="inspector-panel-page" :aria-selected="inspectorTab === 'page'" :class="{ on: inspectorTab === 'page' }" @click="inspectorTab = 'page'">Page</button>
            </div>
            <button type="button" class="ghost small inspector-close" @click="closeInspector"><X :size="14" /> Close</button>
          </header>
          <div class="inspector-content">
            <div v-show="inspectorTab === 'object'" id="inspector-panel-object" role="tabpanel" aria-labelledby="inspector-tab-object">
              <DesignPropertiesPanel :current="current" :selected-id="selectedElementId" :fields="fields" :group-summary="groupSummary" @align="alignSelected" @insert-field="insertField" @replace-image="pickImage" @duplicate="duplicateSelected" @remove="handleDelete" @ungroup="handleUngroup" />
            </div>
            <div v-show="inspectorTab === 'layers'" id="inspector-panel-layers" role="tabpanel" aria-labelledby="inspector-tab-layers">
              <DesignLayersPanel :rows="layerRows" :structure-rows="structureRows" :selected="selected" :move-targets="moveTargets" :can-group="canGroup" :can-ungroup="canUngroup" :has-copied="!!copied" @copy="copySelected" @paste="pasteCopied" @select-node="selectFromStructure" @group="handleGroup" @ungroup="handleUngroup" @move="handleMove" @rename="handleRename" @remove="handleDelete" @reorder="selectAndReorder" />
            </div>
            <div v-show="inspectorTab === 'page'" id="inspector-panel-page" role="tabpanel" aria-labelledby="inspector-tab-page">
              <DesignPagePanel :t="t" :list="list" :quick-sizes="QUICK_SIZES" :footer-options="footerOptions" @set-mode="handleMode" />
            </div>
          </div>
        </aside>
      </div>
    </section>

    <section v-show="mainTab === 'data'" id="editor-panel-data" class="editor-main-panel data-main-panel" role="tabpanel" aria-labelledby="editor-tab-data">
      <DesignPreviewPanel ref="previewPanel" :template="t" :list="list" :fields="fields" :auto-fields="autoFields" :auto-hint="AUTO_HINT" :example-body="exampleBody" />
    </section>

    <section v-show="mainTab === 'api'" id="editor-panel-api" class="editor-main-panel api-main-panel" role="tabpanel" aria-labelledby="editor-tab-api">
      <DesignApiPanel :local-ip="localIp" :server-port="serverPort" :example-body="exampleBody" :list="!!list" />
    </section>

    <dialog v-if="leaveDialogOpen" ref="leaveDialog" class="editor-dialog" aria-labelledby="leave-dialog-title" @cancel.prevent="cancelLeave">
      <h2 id="leave-dialog-title">Save changes before leaving?</h2>
      <div class="row wrap dialog-actions">
        <button type="button" class="primary" :disabled="saving" @click="saveAndLeave">{{ saving ? "Saving…" : "Save and leave" }}</button>
        <button type="button" class="ghost" :disabled="saving" @click="discardAndLeave">Discard</button>
        <button type="button" class="ghost" @click="cancelLeave">Cancel</button>
      </div>
    </dialog>
    <dialog v-if="helpDialogOpen" ref="helpDialog" class="editor-dialog help-dialog" aria-labelledby="help-dialog-title" @cancel.prevent="helpDialogOpen = false">
      <div class="row"><h2 id="help-dialog-title">Design editor help</h2><span class="grow"></span><button type="button" class="icon" aria-label="Close help" @click="helpDialogOpen = false"><X :size="16" /></button></div>
      <ul><li>Double-click to enter a group. Esc to go back.</li><li>Arrow keys move the selected object or group. Shift + arrow moves farther.</li><li>⌘/Ctrl + G groups selected nodes; Shift + ⌘/Ctrl + G ungroups.</li><li>⌘/Ctrl + Z undoes; Shift + ⌘/Ctrl + Z redoes.</li><li>Delete removes the selected object or group.</li></ul>
    </dialog>
  </div>
</template>
