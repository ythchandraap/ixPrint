<script setup lang="ts">
import { Barcode, Image as ImageIcon, Minus, QrCode, Square, Type } from "lucide-vue-next";
import type { Template } from "@/shared/api";
import AutocompleteField from "@/shared/ui/AutocompleteField.vue";
type ListLayout = Extract<NonNullable<Template["layout"]>, { mode: "list" }>;
type ElementType = Exclude<import("@/shared/api").TemplateElement["type"], "image">;
defineProps<{
  t: Template;
  list: ListLayout | null;
  quickSizes: [string, number, number][];
  footerOptions: { value: string; label: string }[];
  imageError: string;
}>();
const emit = defineEmits<{
  "set-mode": [mode: "label" | "list"];
  add: [type: ElementType];
  "choose-image": [];
}>();
</script>
<template>
  <div class="segmented">
    <button type="button" :class="{ on: !list }" @click="emit('set-mode', 'label')">Label · 1 item = 1 page</button>
    <button type="button" :class="{ on: !!list }" @click="emit('set-mode', 'list')">
      List · multiple items per page
    </button>
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
      <label class="field"
        ><span>Header (mm)</span><input v-model.number="list.header_mm" type="number" min="0" step="any"
      /></label>
      <label class="field"
        ><span>Row (mm)</span><input v-model.number="list.row_mm" type="number" min="1" step="any"
      /></label>
      <label class="field"
        ><span>Footer (mm)</span><input v-model.number="list.footer_mm" type="number" min="0" step="any"
      /></label>
    </div>
    <div class="grid2">
      <label class="field"
        ><span>Footer</span>
        <AutocompleteField v-model="list.footer" :options="footerOptions" aria-label="Footer position" />
      </label>
      <label class="row tight check"
        ><input v-model="list.roll" type="checkbox" /> Roll/receipt: height follows content</label
      >
    </div>
  </fieldset>
  <div v-if="!list?.roll" class="row wrap">
    <button
      v-for="[label, w, h] in quickSizes"
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
    <button type="button" class="ghost small" @click="emit('add', 'text')"><Type :size="14" /> Text</button>
    <button type="button" class="ghost small" @click="emit('add', 'line')"><Minus :size="14" /> Line</button>
    <button type="button" class="ghost small" @click="emit('add', 'rect')"><Square :size="14" /> Rectangle</button>
    <button type="button" class="ghost small" @click="emit('add', 'barcode')"><Barcode :size="14" /> Barcode</button>
    <button type="button" class="ghost small" @click="emit('add', 'qr')"><QrCode :size="14" /> QR</button>
    <button type="button" class="ghost small" @click="emit('choose-image')"><ImageIcon :size="14" /> Image</button>
  </div>
  <p v-if="imageError" class="error">⚠ {{ imageError }}</p>
</template>
