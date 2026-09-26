<script setup lang="ts">
import { ref } from "vue";
import { Copy, Trash2 } from "lucide-vue-next";
import type { TemplateElement } from "@/shared/api";
import AutocompleteField from "@/shared/ui/AutocompleteField.vue";
defineProps<{
  current: TemplateElement | null;
  selectedId: string | null;
  fields: string[];
  groupSummary: { name: string; count: number } | null;
}>();
const emit = defineEmits<{
  "insert-field": [field: string];
  "replace-image": [id: string | null];
  duplicate: [];
  remove: [];
  ungroup: [];
  align: [edge: "left" | "hcenter" | "right" | "top" | "vcenter" | "bottom"];
}>();
const newField = ref("");
const selectedAlign = ref("");
const alignOptions = [
  { value: "left", label: "Left" }, { value: "hcenter", label: "Centre horizontally" }, { value: "right", label: "Right" },
  { value: "top", label: "Top" }, { value: "vcenter", label: "Centre vertically" }, { value: "bottom", label: "Bottom" },
];
const alignOptionsText = [
  { value: "left", label: "Left" }, { value: "center", label: "Centre" }, { value: "right", label: "Right" },
];
const valignOptions = [
  { value: "top", label: "Top" }, { value: "middle", label: "Centre" }, { value: "bottom", label: "Bottom" },
];
const fitOptions = [
  { value: "contain", label: "Fit (keep ratio)" }, { value: "stretch", label: "Fill box" },
];
function addField() {
  const field = newField.value.trim().replace(/[^A-Za-z0-9_.-]/g, "_");
  if (!field) return;
  emit("insert-field", field);
  newField.value = "";
}
function useImageField(element: TemplateElement, field: string) {
  if (element.type !== "image") return;
  const normalized = field.trim().replace(/[^A-Za-z0-9_.-]/g, "_");
  element.src = normalized ? `{{${normalized}}}` : "";
}
</script>

<template>
  <div class="object-properties" data-inspector-tab="object">
    <section v-if="groupSummary" class="properties-section group-summary">
      <h3>Group</h3>
      <strong class="group-summary-name" :title="groupSummary.name">{{ groupSummary.name }}</strong>
      <p class="meta">{{ groupSummary.count }} object{{ groupSummary.count === 1 ? "" : "s" }} inside</p>
      <div class="row wrap">
        <button type="button" class="ghost small" @click="emit('ungroup')">Ungroup</button>
        <button type="button" class="icon danger" aria-label="Delete group and its contents" title="Delete group and its contents" @click="emit('remove')"><Trash2 :size="16" /></button>
      </div>
    </section>

    <template v-else-if="current">
      <section class="properties-section">
        <h3>Content</h3>
        <label v-if="current.type === 'text'" class="field">
          <span>Text content</span><textarea v-model="current.text" rows="3" aria-label="Text content"></textarea>
        </label>
        <template v-if="current.type === 'barcode' || current.type === 'qr'">
          <label class="field"><span>{{ current.type === "barcode" ? "Barcode value (Code128, ASCII)" : "QR value (text or URL)" }}</span><input v-model="current.value" /></label>
          <label v-if="current.type === 'barcode'" class="row check"><input v-model="current.show_text" type="checkbox" /> Show text below barcode</label>
        </template>
        <div v-if="current.type === 'image'" class="stack tight">
          <div class="row image-content-row">
            <img v-if="current.src.startsWith('data:')" :src="current.src" class="image-thumb" alt="" />
            <span v-else class="meta truncate" :title="current.src">Data field: <code>{{ current.src || "—" }}</code></span>
            <button type="button" class="ghost small" @click="emit('replace-image', selectedId)">Replace image…</button>
          </div>
          <label class="field"><span>Image data field (optional)</span><input :value="current.src.startsWith('{{') ? current.src.replace(/[{}]/g, '').trim() : ''" placeholder="e.g. photo" @change="useImageField(current, ($event.target as HTMLInputElement).value)" /></label>
        </div>
      </section>

      <section class="properties-section">
        <h3>Appearance</h3>
        <template v-if="current.type === 'text'">
          <div class="property-grid-two">
            <label class="field"><span>Font size (pt)</span><input v-model.number="current.size" type="number" min="2" max="300" step="any" /></label>
            <label class="field"><span>Alignment</span><AutocompleteField v-model="current.align" :options="alignOptionsText" aria-label="Text alignment" /></label>
            <label class="field"><span>Vertical alignment</span><AutocompleteField v-model="current.valign" :options="valignOptions" aria-label="Vertical alignment" /></label>
            <label class="row check"><input v-model="current.bold" type="checkbox" /> Bold</label>
            <label class="row check"><input v-model="current.wrap" type="checkbox" /> Wrap text</label>
            <label class="row check"><input v-model="current.fit" type="checkbox" /> Shrink to fit</label>
          </div>
        </template>
        <template v-if="current.type === 'image'">
          <label class="field"><span>Image placement</span><AutocompleteField v-model="current.fit" :options="fitOptions" aria-label="Image placement" /></label>
          <label class="row check"><input v-model="current.grayscale" type="checkbox" /> Black and white</label>
          <details class="image-format-details">
            <summary class="link-summary">Supported image formats</summary>
            <p class="hint">The editor accepts formats this computer can open, including PNG, JPG, SVG, WebP, GIF, BMP, and AVIF. Image fields are sent as data URLs; supported data URL formats include PNG, JPEG, GIF, BMP, WebP, TIFF, and ICO.</p>
          </details>
        </template>
        <template v-if="current.type === 'rect'">
          <label class="field"><span>Stroke width (pt, 0 = none)</span><input v-model.number="current.stroke" type="number" min="0" step="any" /></label>
          <label class="row check"><input v-model="current.fill" type="checkbox" /> Black fill</label>
        </template>
      </section>

      <section class="properties-section">
        <h3>Geometry</h3>
        <template v-if="current.type === 'line'">
          <div class="property-grid-two">
            <label class="field"><span>X1 (mm)</span><input v-model.number="current.x1" type="number" step="any" /></label>
            <label class="field"><span>Y1 (mm)</span><input v-model.number="current.y1" type="number" step="any" /></label>
            <label class="field"><span>X2 (mm)</span><input v-model.number="current.x2" type="number" step="any" /></label>
            <label class="field"><span>Y2 (mm)</span><input v-model.number="current.y2" type="number" step="any" /></label>
          </div>
          <label class="field"><span>Line width (pt)</span><input v-model.number="current.width" type="number" min="0.1" step="any" /></label>
        </template>
        <div v-else-if="current.type === 'qr'" class="property-grid-two">
          <label class="field"><span>X (mm)</span><input v-model.number="current.x" type="number" step="any" /></label>
          <label class="field"><span>Y (mm)</span><input v-model.number="current.y" type="number" step="any" /></label>
          <label class="field"><span>Size (mm)</span><input v-model.number="current.size" type="number" min="2" step="any" /></label>
        </div>
        <div v-else class="property-grid-two">
          <label class="field"><span>X (mm)</span><input v-model.number="current.x" type="number" step="any" /></label>
          <label class="field"><span>Y (mm)</span><input v-model.number="current.y" type="number" step="any" /></label>
          <label class="field"><span>Width (mm)</span><input v-model.number="current.w" type="number" min="1" step="any" /></label>
          <label class="field"><span>Height (mm)</span><input v-model.number="current.h" type="number" min="1" step="any" /></label>
        </div>
      </section>

      <section class="properties-section">
        <h3>Actions</h3>
        <label class="field"><span>Align to page</span><select v-model="selectedAlign" aria-label="Align selected object to page" @change="emit('align', selectedAlign as 'left' | 'hcenter' | 'right' | 'top' | 'vcenter' | 'bottom'); selectedAlign = ''"><option value="" disabled>Choose alignment</option><option v-for="option in alignOptions" :key="option.value" :value="option.value">{{ option.label }}</option></select></label>
        <div v-if="current.type === 'text' || current.type === 'barcode' || current.type === 'qr'" class="insert-field-control">
          <label class="field" for="insert-template-field"><span>Insert field</span><input id="insert-template-field" v-model="newField" list="template-field-options" placeholder="Choose or add a field" @keydown.enter.prevent="addField" /></label>
          <datalist id="template-field-options"><option v-for="field in fields" :key="field" :value="field" /></datalist>
          <button type="button" class="ghost small" :disabled="!newField.trim()" @click="addField">Insert</button>
        </div>
        <div class="row object-actions">
          <button type="button" class="ghost small" @click="emit('duplicate')"><Copy :size="14" /> Duplicate</button>
          <span class="grow"></span>
          <button type="button" class="icon danger" title="Delete object" aria-label="Delete object" @click="emit('remove')"><Trash2 :size="16" /></button>
        </div>
      </section>
    </template>
    <p v-else class="hint">Select an object to edit its properties.</p>
  </div>
</template>
