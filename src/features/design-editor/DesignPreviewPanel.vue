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
  localIp: string;
  serverPort: number;
}>();
const previewRows = ref(12);
const printPages = ref<string[]>([]);
const printTotal = ref(0);
const printError = ref("");
const printBusy = ref(false);
const sampleJson = ref(props.exampleBody);
const sampleEdited = ref(false);
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
    if (!items.length || items.length > 500) throw new Error("Use 1–500 data items");
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
function toggleRequired(field: string, required: boolean) {
  const next = new Set(props.template.required_fields ?? []);
  if (required) next.add(field);
  else next.delete(field);
  props.template.required_fields = [...next].sort();
}
function useJsonSample() {
  if (parsed.value.error || !parsed.value.records.length) return;
  Object.assign(props.template.sample, parsed.value.header, parsed.value.records[0]);
}
async function showPrintPreview() {
  printBusy.value = true;
  printError.value = "";
  try {
    const rows = props.list ? previewRows.value : Math.min(previewRows.value, 6);
    const [pages, total] = await api.previewPrint(JSON.parse(JSON.stringify(props.template)), rows, 3);
    [printPages.value, printTotal.value] = [pages, total];
  } catch (error) {
    printError.value = errorText(error);
  } finally {
    printBusy.value = false;
  }
}
async function showJsonPreview() {
  if (parsed.value.error || parsed.value.missing.length) return;
  printBusy.value = true;
  printError.value = "";
  try {
    const [pages, total] = await api.previewPrintData(
      JSON.parse(JSON.stringify(props.template)), parsed.value.records, parsed.value.header, 3,
    );
    [printPages.value, printTotal.value] = [pages, total];
  } catch (error) {
    printError.value = errorText(error);
  } finally {
    printBusy.value = false;
  }
}
</script>
<template>
  <fieldset>
    <legend>Sample data</legend>
    <p class="hint">
      Enter example values to see dynamic text in the design preview and test print. New fields start with “Contoh”.
    </p>
    <p v-if="!fields.length" class="hint">
      No fields yet. Write {{ "{" + "{field}" + "}" }} in text, barcode or QR content.
    </p>
    <template v-if="autoFields.length">
      <p class="hint">
        Automatic fields — leave blank for ix.print to fill, or enter your own value (the web app may also send it):
      </p>
      <div class="grid2">
        <label v-for="field in autoFields" :key="field" class="field">
          <span>{{ field }}</span>
          <input v-model="template.sample[field]" :placeholder="`automatic: ${autoHint[field]}`" />
        </label>
      </div>
    </template>
    <p v-else class="hint">
      Available automatic fields: <code>{{ "{" + "{no}" + "}" }}</code
      >, <code>{{ "{" + "{halaman}" + "}" }}</code
      >, <code>{{ "{" + "{total_halaman}" + "}" }}</code
      >, <code>{{ "{" + "{jumlah_data}" + "}" }}</code> — their values can also be provided manually.
    </p>
    <div class="grid2">
      <label v-for="field in fields" :key="field" class="field">
        <span>{{ field }}</span>
        <input v-model="template.sample[field]" placeholder="e.g. Product name" />
        <span class="row tight check"><input type="checkbox" :checked="template.required_fields?.includes(field)" @change="toggleRequired(field, ($event.target as HTMLInputElement).checked)" /> Required for printing</span>
      </label>
    </div>
  </fieldset>

  <fieldset class="stack tight">
    <legend>Real data preview</legend>
    <p class="hint">Paste the JSON body your application will send. Preview renders these records without printing.</p>
    <textarea v-model="sampleJson" rows="7" spellcheck="false" class="json-sample" @input="sampleEdited = true"></textarea>
    <p v-if="parsed.error" class="error">{{ parsed.error }}</p>
    <p v-else-if="parsed.missing.length" class="error">Required values missing: {{ parsed.missing.slice(0, 5).join(", ") }}{{ parsed.missing.length > 5 ? ` (+${parsed.missing.length - 5} more)` : "" }}</p>
    <p v-if="parsed.unused.length" class="hint">Data fields not used by this design: {{ parsed.unused.join(", ") }}</p>
    <div class="row wrap">
      <button type="button" class="ghost small" :disabled="!!parsed.error" @click="useJsonSample">Use first item as canvas sample</button>
      <button type="button" class="ghost small" :disabled="printBusy || !!parsed.error || !!parsed.missing.length" @click="showJsonPreview">
        {{ printBusy ? "Rendering…" : "Preview JSON" }}
      </button>
    </div>
  </fieldset>

  <fieldset>
    <legend>Call from a web app</legend>
    <p class="hint">Send a JSON body (plus the token header when required):</p>
    <pre class="settings">
POST http://{{ localIp }}:{{ serverPort }}/print
Content-Type: application/json
{{ exampleBody }}</pre>
    <p class="hint">
      <code>"data"</code> is an array: one object = {{ list ? "one row" : "one label" }}.
      <code>"header"</code> (optional) contains values shared by every page{{
        list ? " — fields in the Header/Footer bands" : ""
      }}. Other settings go in <code>"settings"</code>, e.g. <code>{"copies": 2}</code>.
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
      <p class="meta">
        {{ printTotal }} page{{ printTotal === 1 ? "" : "s"
        }}{{ printTotal > printPages.length ? ` (${printPages.length} first shown)` : "" }}
      </p>
      <div class="pages">
        <figure v-for="(src, index) in printPages" :key="index">
          <img :src="src" alt="" />
          <figcaption class="meta">p. {{ index + 1 }}</figcaption>
        </figure>
      </div>
    </template>
  </fieldset>
</template>
