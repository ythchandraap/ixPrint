<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import { api, errorText, type Config, type ImportAction, type ImportPreview, type PrinterView } from "@/shared/api";
import { summarize } from "@/shared/lib/preset";
import AutocompleteField from "@/shared/ui/AutocompleteField.vue";

const props = defineProps<{
  source:
    | { kind: "file"; path: string }
    | { kind: "remote"; remoteId: string; remoteName: string; name: string; bundleSha256: string };
  preview: ImportPreview;
  printers: PrinterView[];
}>();

const emit = defineEmits<{
  (e: "done", config: Config, imported: string[]): void;
  (e: "cancel"): void;
}>();

/** Per preset: what to do, optional new name, and printer remap ("keep" = unchanged). */
const rows = reactive(
  props.preview.items.map((item) => ({
    item,
    action: (item.exists ? "skip" : "overwrite") as ImportAction,
    newName: "",
    printer: item.printer_missing ? "" : "keep",
  })),
);
const replaceAll = ref(false);
const includeDefaults = ref(false);
const error = ref("");
const busy = ref(false);

const actionOptions = [
  { value: "overwrite", label: "Overwrite existing / import" },
  { value: "rename", label: "Save under a new name" },
  { value: "skip", label: "Skip" },
];
function settingsSummary(settings: ImportPreview["items"][number]["settings"]) {
  const details = [summarize(settings)];
  if (settings.margins) details.push(`margins ${settings.margins}`);
  if (settings.offset_x || settings.offset_y)
    details.push(`offset ${settings.offset_x ?? 0}, ${settings.offset_y ?? 0} mm`);
  if (settings.tray) details.push(`tray ${settings.tray}`);
  if (settings.dpi) details.push(`${settings.dpi} dpi`);
  return details.join(" · ");
}
function printerOptions(row: (typeof rows)[number]) {
  return [
    {
      value: "keep",
      label: row.item.settings.printer && !row.item.printer_missing ? row.item.settings.printer : "Keep as in file",
    },
    { value: "", label: "— default printer —" },
    ...props.printers.map((p) => ({ value: p.name, label: p.name })),
  ];
}

// Replacing everything removes the existing presets, so "skip" would lose them.
watch(replaceAll, (on) => {
  for (const r of rows) {
    if (on && r.item.exists && r.action === "skip") r.action = "overwrite";
  }
});

async function apply() {
  error.value = "";
  busy.value = true;
  try {
    const options = {
      replace_all: props.source.kind === "file" && replaceAll.value,
      include_defaults: props.source.kind === "file" && includeDefaults.value,
      decisions: rows.map((r) => ({
        name: r.item.name,
        action: r.action,
        new_name: r.action === "rename" && r.newName.trim() ? r.newName.trim() : undefined,
        printer: r.printer === "keep" ? undefined : r.printer,
      })),
    };
    const { config, imported } =
      props.source.kind === "file"
        ? await api.applyImport(props.source.path, options)
        : await api.applyRemotePresetImport(
            props.source.remoteId,
            props.source.name,
            props.source.bundleSha256,
            options,
          );
    emit("done", config, imported);
  } catch (e) {
    error.value = errorText(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="stack">
    <h3 class="section-title">{{ source.kind === "remote" ? "Import from remote" : "Import preset" }}</h3>
    <p v-if="source.kind === 'file'" class="hint truncate" :title="source.path">{{ source.path }}</p>
    <p v-else class="hint">{{ source.remoteName }} · {{ source.name }} · fetched again before import</p>

    <div v-for="row in rows" :key="row.item.name" class="card stack tight">
      <div class="row">
        <strong class="grow truncate">{{ row.item.name }}</strong>
        <span v-if="row.item.exists" class="badge warn">exists</span>
        <span v-else class="badge">new</span>
      </div>
      <div class="meta">{{ settingsSummary(row.item.settings) }}</div>
      <div class="meta">
        Printer: {{ row.item.settings.printer || "default printer" }} · Design:
        {{ row.item.settings.design ? "yes" : "no" }}
      </div>

      <label class="field">
        <span>Action</span>
        <AutocompleteField v-model="row.action" :options="actionOptions" aria-label="Import action" />
      </label>
      <label v-if="row.action === 'rename'" class="field">
        <span>New name</span>
        <input v-model="row.newName" :placeholder="`${row.item.name} (2)`" />
      </label>

      <label v-if="row.action !== 'skip'" class="field">
        <span>
          Printer
          <template v-if="row.item.printer_missing">
            <span class="warn-text">⚠ "{{ row.item.settings.printer }}" is not installed on this computer</span>
          </template>
        </span>
        <AutocompleteField v-model="row.printer" :options="printerOptions(row)" aria-label="Printer import" />
      </label>
    </div>

    <label v-if="source.kind === 'file'" class="row check">
      <input v-model="replaceAll" type="checkbox" />
      <span>Remove all existing presets and replace them with the file contents</span>
    </label>
    <label v-if="source.kind === 'file' && (preview.default_printer || preview.default_preset)" class="row check">
      <input v-model="includeDefaults" type="checkbox" />
      <span>
        Also use the defaults from the file:
        <template v-if="preview.default_preset">preset "{{ preview.default_preset }}"</template>
        <template v-if="preview.default_printer">
          , printer "{{ preview.default_printer }}"
          <span v-if="preview.default_printer_missing" class="warn-text">(not installed)</span>
        </template>
      </span>
    </label>

    <p v-if="error" class="error">{{ error }}</p>
    <div class="row end">
      <button class="ghost" :disabled="busy" @click="emit('cancel')">Cancel</button>
      <button
        class="primary"
        :disabled="
          busy ||
          rows.some((row) => row.action === 'rename' && !row.newName.trim()) ||
          (source.kind === 'remote' && !preview.bundle_sha256)
        "
        @click="apply"
      >
        {{ busy ? "Importing…" : "Import" }}
      </button>
    </div>
  </div>
</template>
