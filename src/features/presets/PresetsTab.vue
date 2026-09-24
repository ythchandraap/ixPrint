<script setup lang="ts">
import { computed, ref } from "vue";
import { confirm, open, save } from "@tauri-apps/plugin-dialog";
import { Download, Pencil, Plus, Printer, SlidersHorizontal, Star, Trash2, Upload } from "lucide-vue-next";
import EmptyState from "@/shared/ui/EmptyState.vue";
import { api, errorText, type Config, type ImportPreview, type PrinterView, type PrintSettings } from "@/shared/api";
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
	| { kind: "import"; path: string; preview: ImportPreview };

const view = ref<View>({ kind: "list" });
const names = computed(() => Object.keys(props.config.presets));
const exportSelection = ref<string[]>([]);
const exportDefaults = ref(true);

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

function saved(config: Config) {
	emit("config", config);
	emit("notify", "Preset saved");
	view.value = { kind: "list" };
}

function startExport() {
	exportSelection.value = [...names.value];
	view.value = { kind: "export" };
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
	const path = await open({ multiple: false, filters: [{ name: "Preset ix.print", extensions: ["json"] }] });
	if (typeof path !== "string") return;
	const preview = await run(() => api.previewImport(path));
	if (preview) view.value = { kind: "import", path, preview };
}

function imported(config: Config, list: string[]) {
	emit("config", config);
	emit("notify", list.length ? `Imported: ${list.join(", ")}` : "No presets imported");
	view.value = { kind: "list" };
}
</script>

<template>
	<PresetForm
		v-if="view.kind === 'edit'"
		:name="view.name"
		:settings="view.settings"
		:printers="printers"
		:local-ip="localIp"
		:server-port="serverPort"
		@saved="saved"
		@cancel="view = { kind: 'list' }"
	/>

	<ImportDialog
		v-else-if="view.kind === 'import'"
		:path="view.path"
		:preview="view.preview"
		:printers="printers"
		@done="imported"
		@cancel="view = { kind: 'list' }"
	/>

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

	<section v-else class="stack">
		<div class="row">
			<button class="primary small" @click="view = { kind: 'edit', settings: {} }"><Plus :size="14" /> New</button>
			<span class="grow"></span>
			<button class="ghost small" title="Import from file" @click="startImport"><Upload :size="14" /> Import</button>
			<button class="ghost small" title="Export to file" :disabled="!names.length" @click="startExport">
				<Download :size="14" /> Export
			</button>
		</div>

		<EmptyState
			v-if="!names.length"
			:icon="SlidersHorizontal"
			title="No presets yet"
			text="Presets store print settings (printer, paper, scale, margins…) so a web app only needs to send ?preset=name. Without a preset, request parameters and defaults are used."
		>
			<button class="primary small" @click="view = { kind: 'edit', settings: {} }"><Plus :size="14" /> Create preset</button>
			<button class="ghost small" @click="startImport"><Upload :size="14" /> Import</button>
		</EmptyState>

		<div v-for="name in names" :key="name" class="card row">
			<div class="grow min0">
				<div class="row tight">
					<strong class="truncate">{{ name }}</strong>
					<span v-if="config.default_preset === name" class="badge">default</span>
					<span v-if="config.presets[name].design" class="badge subtle">design</span>
				</div>
				<div class="meta truncate">{{ config.presets[name].printer ?? "default printer" }}</div>
				<div class="meta truncate">{{ summarize(config.presets[name]) }}</div>
			</div>
			<button class="icon" :title="config.default_preset === name ? 'Remove as default' : 'Set as default'" @click="toggleDefault(name)">
				<Star :size="16" :fill="config.default_preset === name ? 'currentColor' : 'none'" />
			</button>
			<button class="icon" :title="config.presets[name].design ? `Open sample print for this preset (${testPages} pages)` : `Open calibration print for this preset (${testPages} pages)`" @click="emit('open-print', name)"><Printer :size="16" /></button>
			<button class="icon" title="Edit" @click="view = { kind: 'edit', name, settings: config.presets[name] }"><Pencil :size="16" /></button>
			<button class="icon danger" title="Remove" @click="remove(name)"><Trash2 :size="16" /></button>
		</div>
	</section>
</template>
