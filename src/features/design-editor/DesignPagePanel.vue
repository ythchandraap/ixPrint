<script setup lang="ts">
import type { Template } from "@/shared/api";
import AutocompleteField from "@/shared/ui/AutocompleteField.vue";

type ListLayout = Extract<NonNullable<Template["layout"]>, { mode: "list" }>;
defineProps<{
  t: Template;
  list: ListLayout | null;
  quickSizes: [string, number, number][];
  footerOptions: { value: string; label: string }[];
}>();
const emit = defineEmits<{ "set-mode": [mode: "label" | "list"] }>();
</script>

<template>
  <div class="page-panel stack" data-inspector-tab="page">
    <div class="page-mode segmented" role="group" aria-label="Page mode">
      <button type="button" :class="{ on: !list }" :aria-pressed="!list" @click="emit('set-mode', 'label')">Label</button>
      <button type="button" :class="{ on: !!list }" :aria-pressed="!!list" @click="emit('set-mode', 'list')">List</button>
    </div>
    <p class="hint">{{ list ? "Multiple items per page." : "One item per page." }}</p>
    <div class="page-dimensions">
      <label class="field"><span>Page width (mm)</span><input v-model.number="t.width_mm" type="number" min="5" max="2000" step="any" required /></label>
      <label v-if="!list?.roll" class="field"><span>Page height (mm)</span><input v-model.number="t.height_mm" type="number" min="5" max="2000" step="any" required /></label>
    </div>

    <template v-if="list">
      <section class="page-section stack tight">
        <h3>Bands</h3>
        <p class="hint">Header and footer frame the repeated rows.</p>
        <details>
          <summary class="link-summary">Band behavior</summary>
          <p class="hint">The header is printed at the top of each page, the row repeats for each item, and the footer {{ list.footer === "every_page" ? "appears at the bottom of each page" : "appears once after the last row" }}. Elements are assigned to a band according to their canvas position.</p>
        </details>
        <div class="page-bands">
          <label class="field"><span>Header (mm)</span><input v-model.number="list.header_mm" type="number" min="0" step="any" /></label>
          <label class="field"><span>Row (mm)</span><input v-model.number="list.row_mm" type="number" min="1" step="any" /></label>
          <label class="field"><span>Footer (mm)</span><input v-model.number="list.footer_mm" type="number" min="0" step="any" /></label>
        </div>
        <label class="field"><span>Footer placement</span><AutocompleteField v-model="list.footer" :options="footerOptions" aria-label="Footer placement" /></label>
        <label class="row check"><input v-model="list.roll" type="checkbox" /> Roll/receipt: height follows content</label>
      </section>
    </template>

    <section v-if="!list?.roll" class="page-section stack tight">
      <h3>Quick sizes</h3>
      <details class="quick-size-menu">
        <summary class="ghost small">Choose size</summary>
        <div class="quick-size-options">
          <button v-for="[label, width, height] in quickSizes" :key="label" type="button" @click="[t.width_mm, t.height_mm] = [width, height]">{{ label }} mm</button>
        </div>
      </details>
    </section>
  </div>
</template>
