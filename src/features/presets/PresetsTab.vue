<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import {
  Download,
  Globe,
  MoreHorizontal,
  Pencil,
  Plus,
  Printer,
  RefreshCw,
  SlidersHorizontal,
  Star,
  Trash2,
  Upload,
} from "lucide-vue-next";
import EmptyState from "@/shared/ui/EmptyState.vue";
import {
  api,
  errorText,
  type Config,
  type ImportPreview,
  type PrinterView,
  type PrintSettings,
  type RemoteView,
  type SharedPreset,
} from "@/shared/api";
import { summarize } from "@/shared/lib/preset";
import PresetForm from "./PresetForm.vue";
import ImportDialog from "./ImportDialog.vue";

const props = defineProps<{
  config: Config;
  printers: PrinterView[];
  testPages: number;
  localIp: string;
  serverPort: number;
}>();

const emit = defineEmits<{
  (e: "config", config: Config): void;
  (e: "notify", message: string, error?: boolean): void;
  (e: "open-print", preset: string): void;
}>();

type View =
  | { kind: "list" }
  | { kind: "edit"; name?: string; settings: PrintSettings }
  | { kind: "export" }
  | { kind: "remote" }
  | {
      kind: "import";
      source:
        | { kind: "file"; path: string }
        | { kind: "remote"; remoteId: string; remoteName: string; name: string; bundleSha256: string };
      preview: ImportPreview;
    };

const view = ref<View>({ kind: "list" });
const names = computed(() => Object.keys(props.config.presets));
const exportSelection = ref<string[]>([]);
const exportDefaults = ref(true);
const remotes = ref<RemoteView[]>([]);
const selectedRemoteId = ref("");
const sharedRemotePresets = ref<SharedPreset[]>([]);
const remotePresetName = ref("");
const remoteListingLoaded = ref(false);
const remoteListingDisabled = ref(false);
const remoteBusy = ref(false);
const remoteError = ref("");
const shareBusy = ref(new Set<string>());
const actionsOpen = ref(false);
const openPresetActions = ref<string | null>(null);
const importExportTrigger = ref<HTMLButtonElement | null>(null);
const presetActionTriggers = new Map<string, HTMLButtonElement>();
const remoteSource = computed(() => remotes.value.find((remote) => remote.id === selectedRemoteId.value));

async function run<T>(action: () => Promise<T>): Promise<T | undefined> {
  try {
    return await action();
  } catch (e) {
    emit("notify", errorText(e), true);
  }
}

async function toggleDefault(name: string) {
  const next = props.config.default_preset === name ? null : name;
  const config = await run(() => api.setDefaultPreset(next));
  if (config) emit("config", config);
}

async function remove(name: string) {
  if (!(await confirm(`Remove preset "${name}"?`, { title: "ix.print", kind: "warning" }))) return;
  const config = await run(() => api.deletePreset(name));
  if (config) {
    emit("config", config);
    emit("notify", `Preset "${name}" removed`);
  }
}

function saved(config: Config, draft: boolean) {
  emit("config", config);
  emit("notify", draft ? "Draft saved. Test it, then publish when ready." : "Preset created");
  view.value = { kind: "list" };
}

async function publish(name: string) {
  if (
    props.config.shared_presets.includes(name) &&
    !(await confirm(
      `Publishing will immediately make the new version of "${name}" available to computers you have shared it with. Continue?`,
      { title: "Share preset over LAN", kind: "warning" },
    ))
  )
    return;
  const config = await run(() => api.publishPresetDraft(name));
  if (config) {
    emit("config", config);
    emit("notify", `Preset "${name}" published`);
  }
}

async function discard(name: string) {
  const config = await run(() => api.discardPresetDraft(name));
  if (config) {
    emit("config", config);
    emit("notify", `Draft "${name}" discarded`);
  }
}

async function restore(name: string) {
  const config = await run(() => api.restorePresetPrevious(name));
  if (config) {
    emit("config", config);
    emit("notify", `Previous version of "${name}" restored`);
  }
}

async function testDraft(name: string) {
  const result = await run(() => api.testPresetDraft(name));
  if (result) emit("notify", `Draft test submitted · job ${result.job_id}`);
}

function startExport() {
  actionsOpen.value = false;
  exportSelection.value = [...names.value];
  view.value = { kind: "export" };
}

function startNewPreset() {
  actionsOpen.value = false;
  view.value = { kind: "edit", settings: {} };
}

async function doExport() {
  const path = await save({
    defaultPath: "ix.print-presets.json",
    filters: [{ name: "Preset ix.print", extensions: ["json"] }],
  });
  if (!path) return;
  const all = exportSelection.value.length === names.value.length;
  const count = await run(() => api.exportPresets(path, all ? null : exportSelection.value, exportDefaults.value));
  if (count !== undefined) {
    emit("notify", `${count} preset(s) exported`);
    view.value = { kind: "list" };
  }
}

async function startImport() {
  actionsOpen.value = false;
  const path = await open({ multiple: false, filters: [{ name: "Preset ix.print", extensions: ["json"] }] });
  if (typeof path !== "string") return;
  const preview = await run(() => api.previewImport(path));
  if (preview) view.value = { kind: "import", source: { kind: "file", path }, preview };
}

async function startRemoteImport() {
  actionsOpen.value = false;
  const found = await run(() => api.remotes());
  if (!found) return;
  remotes.value = found;
  selectedRemoteId.value = found[0]?.id ?? "";
  sharedRemotePresets.value = [];
  remotePresetName.value = "";
  remoteListingLoaded.value = false;
  remoteListingDisabled.value = false;
  remoteError.value = "";
  view.value = { kind: "remote" };
}

async function loadRemotePresets() {
  if (!selectedRemoteId.value) return;
  remoteError.value = "";
  remoteBusy.value = true;
  try {
    const result = await api.getRemoteSharedPresets(selectedRemoteId.value);
    sharedRemotePresets.value = result.presets;
    remoteListingDisabled.value = !result.listing_enabled;
    remoteListingLoaded.value = true;
    remotePresetName.value = result.presets[0]?.name ?? "";
  } catch (error) {
    remoteError.value = errorText(error);
  } finally {
    remoteBusy.value = false;
  }
}

function resetRemoteSelection() {
  remoteListingLoaded.value = false;
  remoteListingDisabled.value = false;
  remoteError.value = "";
  sharedRemotePresets.value = [];
  remotePresetName.value = "";
}

async function previewRemoteImport() {
  const name = remotePresetName.value.trim();
  if (!selectedRemoteId.value || !name) return;
  remoteError.value = "";
  remoteBusy.value = true;
  try {
    const preview = await api.previewRemotePresetImport(selectedRemoteId.value, name);
    if (!preview.bundle_sha256) throw new Error("Remote preset preview did not include a version hash");
    const remote = remoteSource.value;
    if (!remote) throw new Error("Choose a saved remote computer");
    view.value = {
      kind: "import",
      source: {
        kind: "remote",
        remoteId: remote.id,
        remoteName: remote.name,
        name,
        bundleSha256: preview.bundle_sha256,
      },
      preview,
    };
  } catch (error) {
    remoteError.value = errorText(error);
  } finally {
    remoteBusy.value = false;
  }
}

async function setShared(name: string, shared: boolean) {
  shareBusy.value.add(name);
  try {
    const config = await api.setPresetShared(name, shared);
    emit("config", config);
  } catch (error) {
    emit("notify", errorText(error), true);
  } finally {
    shareBusy.value.delete(name);
    if (openPresetActions.value === name) {
      openPresetActions.value = null;
      nextTick(() => presetActionTriggers.get(name)?.focus());
    }
  }
}

function setPresetActionTrigger(name: string, element: unknown) {
  if (element instanceof HTMLButtonElement) presetActionTriggers.set(name, element);
  else presetActionTriggers.delete(name);
}

function toggleImportExport() {
  openPresetActions.value = null;
  actionsOpen.value = !actionsOpen.value;
}

function togglePresetActions(name: string) {
  actionsOpen.value = false;
  openPresetActions.value = openPresetActions.value === name ? null : name;
}

function closeMenus() {
  actionsOpen.value = false;
  openPresetActions.value = null;
}

function closeMenusOnOutsidePointer(event: PointerEvent) {
  const target = event.target;
  if (target instanceof Element && !target.closest(".preset-action-menu, .preset-row-action-menu")) {
    closeMenus();
  }
}

onMounted(() => document.addEventListener("pointerdown", closeMenusOnOutsidePointer));
onBeforeUnmount(() => document.removeEventListener("pointerdown", closeMenusOnOutsidePointer));

function closeMenusOnEscape() {
  const presetName = openPresetActions.value;
  if (presetName) {
    openPresetActions.value = null;
    nextTick(() => presetActionTriggers.get(presetName)?.focus());
    return;
  }
  if (actionsOpen.value) {
    actionsOpen.value = false;
    nextTick(() => importExportTrigger.value?.focus());
  }
}

function runPresetAction(name: string, action: () => void) {
  openPresetActions.value = null;
  nextTick(() => presetActionTriggers.get(name)?.focus());
  action();
}

function imported(config: Config, list: string[]) {
  emit("config", config);
  emit("notify", list.length ? `Imported: ${list.join(", ")}` : "No presets imported");
  view.value = { kind: "list" };
}
</script>

<template>
  <Transition name="detail" mode="out-in">
  <PresetForm
    v-if="view.kind === 'edit'"
    :name="view.name"
    :settings="view.name ? (config.preset_drafts[view.name] ?? view.settings) : view.settings"
    :printers="printers"
    :local-ip="localIp"
    :server-port="serverPort"
    @saved="saved"
    @cancel="view = { kind: 'list' }"
  >
    <template #design-editor="editorProps">
      <slot name="design-editor" v-bind="editorProps" />
    </template>
  </PresetForm>

  <ImportDialog
    v-else-if="view.kind === 'import'"
    :source="view.source"
    :preview="view.preview"
    :printers="printers"
    @done="imported"
    @cancel="view = { kind: 'list' }"
  />

  <section v-else-if="view.kind === 'remote'" class="stack">
    <h3 class="section-title">Import from remote</h3>
    <label class="field">
      <span>Source computer</span>
      <select v-model="selectedRemoteId" :disabled="!remotes.length || remoteBusy" @change="resetRemoteSelection">
        <option value="" disabled>Select a saved remote</option>
        <option v-for="remote in remotes" :key="remote.id" :value="remote.id">{{ remote.name }}</option>
      </select>
    </label>

    <p v-if="!remotes.length" class="hint">Add a computer in Settings → Other ix.print computers first.</p>
    <div v-else class="row">
      <button class="primary small" :disabled="remoteBusy || !selectedRemoteId" @click="loadRemotePresets">
        <RefreshCw :size="14" />
        {{ remoteBusy ? "Loading…" : remoteListingLoaded ? "Refresh shared presets" : "Load shared presets" }}
      </button>
    </div>

    <p v-if="remoteListingDisabled" class="info-note">
      The source computer has disabled shared preset listing. Enter the preset name to request it directly.
    </p>
    <label v-if="remoteListingDisabled" class="field">
      <span>Preset name</span>
      <input v-model="remotePresetName" maxlength="64" autocomplete="off" placeholder="Published preset name" />
    </label>
    <template v-else-if="remoteListingLoaded && sharedRemotePresets.length">
      <label class="field">
        <span>Shared preset</span>
        <select v-model="remotePresetName">
          <option v-for="preset in sharedRemotePresets" :key="preset.name" :value="preset.name">
            {{ preset.name }}{{ preset.has_design ? " · has design" : "" }}
          </option>
        </select>
      </label>
    </template>
    <p v-else-if="remoteListingLoaded" class="hint">No presets are currently shared by this computer.</p>

    <p v-if="remoteError" class="error">{{ remoteError }}</p>
    <div class="row end">
      <button class="ghost" :disabled="remoteBusy" @click="view = { kind: 'list' }">Cancel</button>
      <button
        v-if="remoteListingDisabled || (remoteListingLoaded && sharedRemotePresets.length)"
        class="primary"
        :disabled="remoteBusy || !selectedRemoteId || !remotePresetName.trim()"
        @click="previewRemoteImport"
      >
        {{ remoteBusy ? "Loading preview…" : "Preview import" }}
      </button>
    </div>
  </section>

  <section v-else-if="view.kind === 'export'" class="stack">
    <h3 class="section-title">Export preset</h3>
    <label v-for="name in names" :key="name" class="row check card">
      <input v-model="exportSelection" type="checkbox" :value="name" />
      <span class="grow truncate">{{ name }}</span>
    </label>
    <label class="row check">
      <input v-model="exportDefaults" type="checkbox" />
      <span>Include the default printer &amp; default preset</span>
    </label>
    <div class="row end">
      <button class="ghost" @click="view = { kind: 'list' }">Cancel</button>
      <button class="primary" :disabled="!exportSelection.length" @click="doExport">Save file…</button>
    </div>
  </section>

  <section v-else class="stack presets-view" @click="closeMenus" @keydown.esc.stop.prevent="closeMenusOnEscape">
    <div class="row preset-toolbar">
      <button type="button" class="primary small" @click="startNewPreset"><Plus :size="14" /> New preset</button>
      <div class="preset-action-menu" @click.stop>
        <button
          type="button"
          class="ghost small action-trigger"
          ref="importExportTrigger"
          :aria-expanded="actionsOpen"
          aria-controls="preset-actions-menu"
          @click="toggleImportExport"
        >
          <MoreHorizontal :size="16" />
          <span>Import / Export</span>
        </button>
        <Transition name="popover">
          <div v-if="actionsOpen" id="preset-actions-menu" class="preset-action-popover">
            <button type="button" class="menu-action" @click="startImport">
              <Upload :size="16" />
              <span>Import from file</span>
            </button>
            <button type="button" class="menu-action" @click="startRemoteImport">
              <Download :size="16" />
              <span>Import from remote</span>
            </button>
            <button type="button" class="menu-action" :disabled="!names.length" @click="startExport">
              <Download :size="16" />
              <span>Export to file</span>
            </button>
          </div>
        </Transition>
      </div>
    </div>

    <EmptyState
      v-if="!names.length"
      :icon="SlidersHorizontal"
      title="No presets yet"
      text="Presets store print settings (printer, paper, scale, margins…) so a web app only needs to send ?preset=name. Without a preset, request parameters and defaults are used."
    />

    <div v-else class="preset-list">
      <article v-for="(name, index) in names" :key="name" class="card preset-entry">
        <div class="preset-title-row">
          <strong class="preset-name">{{ name }}</strong>
          <span v-if="config.default_preset === name" class="badge">default</span>
          <span v-if="config.presets[name].design" class="badge subtle">design</span>
          <span v-if="config.preset_drafts[name]" class="badge">draft</span>
          <span class="preset-share-status" :class="{ active: config.shared_presets.includes(name) }">
            <i class="dot" :class="config.shared_presets.includes(name) ? 'ok' : 'idle'"></i>
            {{ config.shared_presets.includes(name) ? "Shared" : "Private" }}
          </span>
        </div>
        <div class="preset-printer-row">
          <span class="meta">Printer</span>
          <span class="preset-printer">
            {{ config.presets[name].printer ?? config.default_printer ?? "default printer" }}
          </span>
        </div>
        <div v-if="config.presets[name].design" class="preset-design-size">
          Design · {{ config.presets[name].design.width_mm }} × {{ config.presets[name].design.height_mm }} mm
        </div>
        <div v-else class="preset-options">{{ summarize(config.presets[name]) }}</div>
        <div class="preset-entry-actions">
          <button
            type="button"
            class="ghost small"
            @click="view = { kind: 'edit', name, settings: config.presets[name] }"
          >
            <Pencil :size="16" /> Edit
          </button>
          <button type="button" class="ghost small" @click="emit('open-print', name)">
            <Printer :size="16" /> Test print…
          </button>
          <div class="preset-row-action-menu" @click.stop>
            <button
              :ref="(element) => setPresetActionTrigger(name, element)"
              type="button"
              class="ghost small preset-more-action"
              :aria-label="`More actions for ${name}`"
              :title="`More actions for ${name}`"
              :aria-expanded="openPresetActions === name"
              :aria-controls="`preset-actions-${index}`"
              @click="togglePresetActions(name)"
            >
              <MoreHorizontal :size="16" />
            </button>
            <Transition name="popover">
              <div v-if="openPresetActions === name" :id="`preset-actions-${index}`" class="preset-row-action-popover">
                <button type="button" class="menu-action" @click="runPresetAction(name, () => toggleDefault(name))">
                  <Star :size="16" :fill="config.default_preset === name ? 'currentColor' : 'none'" />
                  <span>{{ config.default_preset === name ? "Remove as default" : "Set as default" }}</span>
                </button>
                <button
                  type="button"
                  class="menu-action"
                  :disabled="shareBusy.has(name)"
                  @click="setShared(name, !config.shared_presets.includes(name))"
                >
                  <Globe :size="16" />
                  <span v-if="shareBusy.has(name)">Updating sharing…</span>
                  <span v-else>{{ config.shared_presets.includes(name) ? "Stop sharing" : "Share on LAN" }}</span>
                </button>
                <button type="button" class="menu-action danger-text" @click="runPresetAction(name, () => remove(name))">
                  <Trash2 :size="16" />
                  <span>Remove preset</span>
                </button>
              </div>
            </Transition>
          </div>
        </div>
        <p v-if="config.shared_presets.includes(name) && config.preset_drafts[name]" class="hint">
          The published version is available over LAN. Draft changes remain private until published.
        </p>
        <div v-if="config.preset_drafts[name] || config.preset_previous[name]" class="row wrap preset-draft-actions">
          <template v-if="config.preset_drafts[name]">
            <button class="ghost small" @click="testDraft(name)">Print draft sample</button>
            <button class="ghost small" @click="publish(name)">Publish draft</button>
            <button class="ghost small" @click="discard(name)">Discard draft</button>
          </template>
          <button v-if="config.preset_previous[name]" class="ghost small" @click="restore(name)">Restore previous</button>
        </div>
      </article>
    </div>

  </section>
  </Transition>
</template>

<style scoped>
.preset-toolbar {
  justify-content: space-between;
}

.preset-toolbar > button,
.action-trigger {
  min-height: var(--compact-control-height);
}

.preset-action-menu {
  position: relative;
  flex: none;
}

.preset-row-action-menu {
  position: relative;
  flex: none;
}

.action-trigger {
  flex: none;
}

.preset-action-popover {
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: 5;
  min-width: 210px;
  padding: 4px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
}

.preset-row-action-popover {
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: 5;
  min-width: 220px;
  padding: 4px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
}

.menu-action {
  width: 100%;
  min-height: var(--icon-control-size);
  padding: 6px 8px;
  display: flex;
  align-items: center;
  gap: 8px;
  border: none;
  border-radius: var(--r-sm);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 14px;
  text-align: left;
  cursor: pointer;
}

.menu-action:hover:not(:disabled) {
  background: var(--brand-pale);
}

.preset-list {
  display: flex;
  flex-direction: column;
  gap: var(--stack-gap);
}

.preset-entry {
  display: flex;
  flex-direction: column;
  gap: var(--row-gap);
}

.preset-title-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--row-gap);
  min-width: 0;
}

.preset-name,
.preset-printer,
.preset-options,
.preset-design-size {
  min-width: 0;
  overflow-wrap: anywhere;
}

.preset-name {
  font-weight: 500;
}

.preset-share-status {
  margin-left: auto;
}

.preset-printer-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: baseline;
  column-gap: var(--row-gap);
  min-width: 0;
}

.preset-printer {
  color: var(--text);
  line-height: 1.5;
}

.preset-entry-actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--row-gap);
}

.preset-entry-actions > button {
  min-height: var(--compact-control-height);
  flex: 0 0 auto;
}

.preset-more-action {
  justify-content: center;
  width: var(--compact-control-height);
  height: var(--compact-control-height);
  min-width: var(--compact-control-height);
  padding: 0;
}

.preset-options,
.preset-design-size {
  color: var(--muted);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 18px;
}

.preset-share-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--muted);
  font-family: var(--font-mono);
  font-size: 12px;
  letter-spacing: 0.03em;
}

.preset-share-status.active {
  color: var(--ok);
}

.preset-draft-actions > button {
  min-height: var(--control-height);
}

</style>
