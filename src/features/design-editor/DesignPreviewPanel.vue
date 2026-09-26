<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { api, errorText, type Template } from "@/shared/api";
type ListLayout = Extract<NonNullable<Template["layout"]>, { mode: "list" }>;
const props = defineProps<{
  template: Template;
  list: ListLayout | null;
  fields: string[];
  autoFields: string[];
  autoHint: Record<string, string>;
  exampleBody: string;
}>();
const source = ref<"sample" | "json">("sample");
const viewMode = ref<"input" | "result">("input");
const previewRows = ref(12);
const printPages = ref<string[]>([]);
const printTotal = ref(0);
const printError = ref("");
const printBusy = ref(false);
const sampleJson = ref(props.exampleBody);
const sampleEdited = ref(false);
const previewPage = ref(0);
const previewSnapshot = ref("");
let previewRequest = 0;
watch(() => props.exampleBody, (body) => {
  if (!sampleEdited.value) sampleJson.value = body;
});
function asRecord(value: unknown, label: string): Record<string, string> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`${label} must be a JSON object`);
  return Object.fromEntries(Object.entries(value).map(([key, entry]) => {
    if (entry !== null && !["string", "number", "boolean"].includes(typeof entry)) {
      throw new Error(`${label}.${key} must be text, a number, or a boolean`);
    }
    return [key, entry === null ? "" : String(entry)];
  }));
}
const parsed = computed(() => {
  try {
    const value: unknown = JSON.parse(sampleJson.value);
    const body = value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
    const raw = body && "data" in body ? body.data : value;
    const items = Array.isArray(raw) ? raw : [raw];
    if (!items.length || items.length > 500) throw new Error("Use 1–500 records");
    const records = items.map((item, index) => asRecord(item, `data[${index + 1}]`));
    const header = body && "header" in body ? asRecord(body.header, "header") : {};
    const missing = records.flatMap((record, index) =>
      (props.template.required_fields ?? [])
        .filter((field) => !(record[field] || header[field])?.trim())
        .map((field) => `data[${index + 1}].${field}`),
    );
    const unused = [...new Set([...Object.keys(header), ...records.flatMap(Object.keys)])]
      .filter((field) => !props.fields.includes(field) && !props.autoFields.includes(field));
    return { records, header, missing, unused, error: "" };
  } catch (error) {
    return { records: [] as Record<string, string>[], header: {} as Record<string, string>, missing: [] as string[], unused: [] as string[], error: errorText(error) };
  }
});
const previewSignature = computed(() => JSON.stringify({
  template: props.template,
  source: source.value,
  json: source.value === "json" ? sampleJson.value : "",
  rows: source.value === "sample" ? previewRows.value : null,
}));
const previewOutOfDate = computed(() => !!previewSnapshot.value && previewSnapshot.value !== previewSignature.value);
function toggleRequired(field: string, required: boolean) {
  const next = new Set(props.template.required_fields ?? []);
  if (required) next.add(field);
  else next.delete(field);
  props.template.required_fields = [...next].sort();
}
function useFirstRecord() {
  if (parsed.value.error || !parsed.value.records.length) return;
  Object.assign(props.template.sample, parsed.value.header, parsed.value.records[0]);
}
async function previewCurrent() {
  if (source.value === "json" && (parsed.value.error || parsed.value.missing.length)) return;
  if (source.value === "sample" && (!Number.isInteger(previewRows.value) || previewRows.value < 1 || previewRows.value > 500)) {
    printError.value = "Use 1–500 sample items";
    return;
  }
  const request = ++previewRequest;
  const signature = previewSignature.value;
  printBusy.value = true;
  printError.value = "";
  try {
    const result = source.value === "json"
      ? await api.previewPrintData(JSON.parse(JSON.stringify(props.template)), parsed.value.records, parsed.value.header, 3)
      : await api.previewPrint(JSON.parse(JSON.stringify(props.template)), props.list ? previewRows.value : Math.min(previewRows.value, 6), 3);
    if (request !== previewRequest) return;
    [printPages.value, printTotal.value] = result;
    previewPage.value = 0;
    previewSnapshot.value = signature;
    viewMode.value = "result";
  } catch (error) {
    if (request === previewRequest) {
      printError.value = errorText(error);
      viewMode.value = "input";
    }
  } finally {
    if (request === previewRequest) printBusy.value = false;
  }
}
function movePreviewPage(delta: -1 | 1) {
  previewPage.value = Math.max(0, Math.min(printPages.value.length - 1, previewPage.value + delta));
}
defineExpose({ previewCurrent });
</script>

<template>
  <section class="data-preview" :data-view-mode="viewMode" aria-label="Data and print preview">
    <div class="data-preview-mobile-tabs" role="tablist" aria-label="Preview pane">
      <button type="button" role="tab" :aria-selected="viewMode === 'input'" @click="viewMode = 'input'">Input</button>
      <button type="button" role="tab" :aria-selected="viewMode === 'result'" @click="viewMode = 'result'">Result</button>
    </div>
    <section class="data-input-pane" aria-label="Preview input">
      <div class="preview-source-tabs" role="tablist" aria-label="Data source">
        <button type="button" role="tab" :aria-selected="source === 'sample'" @click="source = 'sample'">Sample values</button>
        <button type="button" role="tab" :aria-selected="source === 'json'" @click="source = 'json'">JSON payload</button>
      </div>

      <div v-show="source === 'sample'" class="sample-input stack">
        <div class="sample-input-heading">
          <h2>Sample values</h2>
          <p class="hint">Values used in the canvas and sample preview.</p>
        </div>
        <p v-if="!fields.length" class="hint">No fields yet. Add <code>{{ "{" + "{field}" + "}" }}</code> to text, barcode, or QR content.</p>
        <details v-if="autoFields.length" class="automatic-fields">
          <summary>Automatic fields ({{ autoFields.length }})</summary>
          <div class="automatic-field-list">
            <label v-for="field in autoFields" :key="field" class="field">
              <span>{{ field }}</span><input v-model="template.sample[field]" :placeholder="`automatic: ${autoHint[field]}`" />
            </label>
          </div>
        </details>
        <div class="sample-table" role="table" aria-label="Sample values and required fields">
          <div class="sample-table-head" role="row">
            <span role="columnheader">Field</span><span role="columnheader">Sample value</span><span role="columnheader">Required</span>
          </div>
          <div v-for="field in fields" :key="field" class="sample-field-row" role="row">
            <span class="sample-field-name" role="rowheader">{{ field }}</span>
            <input v-model="template.sample[field]" role="cell" :aria-label="`Sample value for ${field}`" placeholder="e.g. Product name" />
            <label class="sample-required" role="cell"><input type="checkbox" :checked="template.required_fields?.includes(field)" :aria-label="`Required for printing: ${field}`" @change="toggleRequired(field, ($event.target as HTMLInputElement).checked)" /> Required</label>
          </div>
        </div>
        <div class="sample-preview-controls">
          <label class="field"><span>Sample items</span><input v-model.number="previewRows" type="number" min="1" max="500" /></label>
          <button type="button" class="primary small" :disabled="printBusy" @click="previewCurrent">{{ printBusy ? "Rendering…" : "Preview" }}</button>
        </div>
      </div>

      <div v-show="source === 'json'" class="json-input stack">
        <div class="sample-input-heading">
          <h2>JSON payload</h2>
          <p class="hint">Paste request data to preview it without printing.</p>
        </div>
        <textarea v-model="sampleJson" spellcheck="false" class="json-sample" aria-label="JSON payload" @input="sampleEdited = true"></textarea>
        <p v-if="parsed.error" class="error" role="alert">{{ parsed.error }}</p>
        <p v-else-if="parsed.missing.length" class="error" role="alert">Required values missing: {{ parsed.missing.slice(0, 5).join(", ") }}{{ parsed.missing.length > 5 ? ` (+${parsed.missing.length - 5} more)` : "" }}</p>
        <p v-if="parsed.unused.length" class="hint">Unused fields: {{ parsed.unused.join(", ") }}</p>
        <div class="row wrap">
          <button type="button" class="ghost small" :disabled="!!parsed.error" @click="useFirstRecord">Use first record</button>
          <button type="button" class="primary small" :disabled="printBusy || !!parsed.error || !!parsed.missing.length" @click="previewCurrent">{{ printBusy ? "Rendering…" : "Preview" }}</button>
        </div>
      </div>
      <p v-if="printBusy" class="muted input-preview-status" role="status">Rendering…</p>
      <div v-if="printError" class="inline-preview-error"><p class="error" role="alert">{{ printError }}</p><button type="button" class="ghost small" @click="previewCurrent">Retry</button></div>
    </section>

    <section class="preview-result-pane" aria-label="Rendered result" aria-live="polite">
      <div class="preview-result-heading">
        <div><h2>Result</h2><p v-if="previewOutOfDate" class="preview-stale">Preview out of date</p></div>
        <button v-if="previewOutOfDate" type="button" class="ghost small" :disabled="printBusy" @click="previewCurrent">Refresh</button>
      </div>
      <p v-if="printBusy" role="status" class="muted">Rendering…</p>
      <template v-else-if="printError">
        <p class="error" role="alert">{{ printError }}</p>
        <button type="button" class="ghost small" @click="previewCurrent">Retry</button>
      </template>
      <template v-else-if="printPages.length">
        <div class="preview-page-controls">
          <button type="button" class="icon" aria-label="Previous preview page" :disabled="previewPage === 0" @click="movePreviewPage(-1)">‹</button>
          <span class="mono">Page {{ previewPage + 1 }} of {{ printTotal }}</span>
          <button type="button" class="icon" aria-label="Next preview page" :disabled="previewPage >= printPages.length - 1" @click="movePreviewPage(1)">›</button>
        </div>
        <p v-if="printTotal > printPages.length" class="meta">Showing first {{ printPages.length }} pages.</p>
        <div class="preview-page-image"><img :src="printPages[previewPage]" :alt="`Preview page ${previewPage + 1}`" /></div>
      </template>
      <p v-else class="muted">Generate a preview to see the print output.</p>
    </section>
  </section>
</template>
