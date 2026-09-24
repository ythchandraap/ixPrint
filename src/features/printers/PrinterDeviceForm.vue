<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { MoreHorizontal, RefreshCw, Star, Trash2 } from "lucide-vue-next";
import {
	api,
	errorText,
	type Config,
	type NetProtocol,
	type PrinterDetails,
	type PrinterDevice,
	type PrinterDriver,
} from "@/shared/api";
import AutocompleteField from "@/shared/ui/AutocompleteField.vue";

const props = defineProps<{
	/** Existing printer to edit; undefined = add a new one. */
	name?: string;
	os: string;
	config: Config;
}>();

const emit = defineEmits<{
	(e: "done", message: string, config?: Config): void;
	(e: "cancel"): void;
}>();

const isWindows = computed(() => props.os === "windows");
const editing = computed(() => props.name !== undefined);
const busy = ref(false);
const error = ref("");

// ---- add ------------------------------------------------------------------
const form = reactive({
	name: "",
	source: "detected" as "detected" | "network" | "uri",
	deviceUri: "",
	host: "",
	protocol: "ipp" as NetProtocol,
	port: "" as string | number,
	queue: "",
	manualUri: "",
	driverMode: "auto" as "auto" | "pick",
	driver: "",
	description: "",
	location: "",
	shared: false,
});

const devices = ref<PrinterDevice[]>([]);
const devicesLoading = ref(false);
const devicesError = ref("");
async function loadDevices() {
	devicesLoading.value = true;
	devicesError.value = "";
	try {
		devices.value = await api.printerDevices();
	} catch (e) {
		devicesError.value = errorText(e);
	} finally {
		devicesLoading.value = false;
	}
}

const driverQuery = ref("");
const drivers = ref<PrinterDriver[]>([]);
const protocolOptions = [
	{ value: "ipp", label: "IPP (recommended)" },
	{ value: "raw", label: "RAW / JetDirect" },
	{ value: "lpd", label: "LPD" },
];
const driverOptions = computed(() => drivers.value.map((d) => ({ value: d.id, label: d.name })));
let driverTimer: number | undefined;
async function loadDrivers() {
	try {
		drivers.value = await api.printerDrivers(driverQuery.value);
	} catch (e) {
		error.value = errorText(e);
	}
}
watch(driverQuery, () => {
	clearTimeout(driverTimer);
	driverTimer = window.setTimeout(loadDrivers, 250);
});

/** Windows always needs a driver unless the printer is added over IPP. */
const driverRequired = computed(
	() => isWindows.value && !(form.source === "network" && form.protocol === "ipp" && form.driverMode === "auto"),
);
watch(driverRequired, (req) => {
	if (req) form.driverMode = "pick";
});

function pickDevice(d: PrinterDevice) {
	form.deviceUri = d.uri;
	if (!form.name) {
		const base = d.info.replace(/[^A-Za-z0-9_.-]+/g, "_").replace(/^_+|_+$/g, "").slice(0, 60);
		form.name = base || "Printer";
	}
	if (!form.description) form.description = d.info;
	if (!isWindows.value && !driverQuery.value) {
		// Suggest drivers from the make, e.g. "Zebra".
		driverQuery.value = d.info.split(/\s+/)[0] ?? "";
	}
}

// ---- edit -----------------------------------------------------------------
const details = ref<PrinterDetails | null>(null);
const edit = reactive({ name: "", description: "", location: "", shared: false });
const actionsOpen = ref(false);

/** Presets and templates that refer to this printer by name. */
const usedBy = computed(() => {
	if (!props.name) return [];
	const list = Object.entries(props.config.presets)
		.filter(([, s]) => s.printer === props.name)
			.map(([n]) => `preset “${n}”`);
	if (props.config.default_printer === props.name) list.unshift("ix.print default printer");
	return list;
});

onMounted(async () => {
	if (props.name !== undefined) {
		try {
			const d = await api.printerDetails(props.name);
			details.value = d;
			Object.assign(edit, {
				name: d.name,
				description: d.description ?? "",
				location: d.location ?? "",
				shared: d.shared,
			});
		} catch (e) {
			error.value = errorText(e);
		}
	} else {
		loadDevices();
		loadDrivers();
	}
});

async function submit() {
	error.value = "";
	busy.value = true;
	try {
		if (editing.value && props.name !== undefined) {
			const config = await api.printerUpdate(props.name, {
				name: edit.name.trim(),
				description: edit.description.trim() || undefined,
				location: edit.location.trim() || undefined,
				shared: edit.shared,
			});
			emit("done", `Printer “${edit.name.trim()}” updated`, config);
		} else {
			const connection =
				form.source === "network"
					? {
							kind: "network" as const,
							host: form.host.trim(),
							protocol: form.protocol,
							port: form.port === "" ? undefined : Number(form.port),
							queue: form.queue.trim() || undefined,
						}
					: { kind: "device" as const, uri: (form.source === "uri" ? form.manualUri : form.deviceUri).trim() };
			if (connection.kind === "device" && !connection.uri) throw "Choose a device first";
			if (form.driverMode === "pick" && !form.driver) throw "Choose a driver first";
			await api.printerAdd({
				name: form.name.trim(),
				connection,
				driver: form.driverMode === "pick" ? form.driver : undefined,
				description: form.description.trim() || undefined,
				location: form.location.trim() || undefined,
				shared: form.shared,
			});
			emit("done", `Printer “${form.name.trim()}” added`);
		}
	} catch (e) {
		error.value = errorText(e);
	} finally {
		busy.value = false;
	}
}

async function remove() {
	if (!props.name) return;
	error.value = "";
	busy.value = true;
	try {
		const warning = usedBy.value.length ? `\n\nStill used by: ${usedBy.value.join(", ")}.` : "";
		// Inside try: a dialog failure must show up instead of silently doing nothing.
		const ok = await confirm(
			`Remove printer “${props.name}” from the operating system? This printer will also disappear for other applications.${warning}`,
			{ title: "Remove printer", kind: "warning" },
		);
		if (!ok) return;
		// The backend re-checks the OS printer list and fails if it is still there.
		const config = await api.printerRemove(props.name);
		emit("done", `Printer “${props.name}” removed from the system`, config);
	} catch (e) {
		error.value = `Could not remove: ${errorText(e)}`;
	} finally {
		busy.value = false;
	}
}

async function makeOsDefault() {
	if (!props.name) return;
	try {
		await api.printerSetOsDefault(props.name);
		emit("done", `“${props.name}” is now the system default printer`);
	} catch (e) {
		error.value = errorText(e);
	}
}

async function makeOsDefaultFromMenu() {
	actionsOpen.value = false;
	await makeOsDefault();
}

async function removeFromMenu() {
	actionsOpen.value = false;
	await remove();
}

</script>

<template>
	<form class="stack" @submit.prevent="submit">
		<h3 class="section-title">{{ editing ? `Printer “${name}”` : "Add printer" }}</h3>
		<p class="hint">
			Changes the printer in the operating system (this also affects other applications).
			{{ isWindows ? "ix.print usually needs to be run as an administrator." : "An administrator account is required." }}
		</p>

		<!-- ============ EDIT ============ -->
		<template v-if="editing">
			<p v-if="!details && !error" class="muted">Loading…</p>
			<template v-if="details">
				<div class="printer-name-row">
					<label class="field grow" for="edit-printer-name">
						<span>Name{{ details.can_rename ? "" : " (cannot be changed on macOS/Linux)" }}</span>
						<input id="edit-printer-name" v-model="edit.name" :disabled="!details.can_rename" required maxlength="127" />
					</label>
					<div class="action-menu" @click.stop>
						<button
							type="button"
							class="ghost small action-trigger"
							:aria-expanded="actionsOpen"
							aria-haspopup="menu"
							@click="actionsOpen = !actionsOpen"
						>
							<MoreHorizontal :size="16" />
								<span>Actions</span>
						</button>
						<div v-if="actionsOpen" class="action-popover" role="menu">
							<button type="button" class="menu-action" :disabled="busy" role="menuitem" @click="makeOsDefaultFromMenu">
								<Star :size="16" />
								<span>Set as system default</span>
							</button>
							<button type="button" class="menu-action danger" :disabled="busy" role="menuitem" @click="removeFromMenu">
								<Trash2 :size="16" />
								<span>Remove printer</span>
							</button>
						</div>
					</div>
				</div>
				<div class="grid2">
					<label class="field"><span>Description</span><input v-model="edit.description" maxlength="200" /></label>
					<label class="field"><span>Location</span><input v-model="edit.location" maxlength="200" /></label>
				</div>
				<label class="row tight check"><input v-model="edit.shared" type="checkbox" /> Share on the network</label>
				<div class="card stack tight">
					<div class="meta">Connection: <code>{{ details.connection ?? "—" }}</code></div>
					<div class="meta">Driver: {{ details.driver ?? "—" }}</div>
				</div>
				<p v-if="usedBy.length && details.can_rename" class="hint">
					Renaming also updates {{ usedBy.join(", ") }}.
				</p>
			</template>
		</template>

		<!-- ============ ADD ============ -->
		<template v-else>
			<div class="segmented">
				<button type="button" :class="{ on: form.source === 'detected' }" @click="form.source = 'detected'">Detected</button>
				<button type="button" :class="{ on: form.source === 'network' }" @click="form.source = 'network'">IP address</button>
				<button type="button" :class="{ on: form.source === 'uri' }" @click="form.source = 'uri'">Manual {{ isWindows ? "port" : "URI" }}</button>
			</div>

			<fieldset v-if="form.source === 'detected'">
				<legend>Device</legend>
				<div class="row">
					<span class="meta grow">{{ devicesLoading ? "Searching for devices (about 8 seconds)…" : `${devices.length} found` }}</span>
					<button type="button" class="icon" title="Search again" :disabled="devicesLoading" @click="loadDevices"><RefreshCw :size="16" /></button>
				</div>
				<p v-if="devicesError" class="error">{{ devicesError }}</p>
				<label v-for="d in devices" :key="d.uri" :class="['row', 'card', 'check', { active: form.deviceUri === d.uri }]">
					<input type="radio" name="device" :checked="form.deviceUri === d.uri" @change="pickDevice(d)" />
					<div class="grow min0">
						<div class="truncate strong">{{ d.info }}</div>
						<div class="meta truncate">{{ d.kind }} · {{ d.uri }}</div>
					</div>
				</label>
				<p v-if="!devicesLoading && !devices.length && !devicesError" class="hint">
					No devices detected. Make sure the printer is switched on and connected, or use “IP address”.
				</p>
			</fieldset>

			<fieldset v-else-if="form.source === 'network'">
				<legend>Network printer</legend>
				<div class="grid2">
					<label class="field"><span>IP / hostname</span><input v-model="form.host" required placeholder="192.168.1.50" /></label>
					<label class="field"><span>Protocol</span>
						<AutocompleteField v-model="form.protocol" :options="protocolOptions" aria-label="Printer protocol" />
					</label>
				</div>
				<div class="grid2">
					<label class="field"><span>Port</span>
						<input v-model="form.port" type="number" min="1" max="65535" :placeholder="form.protocol === 'ipp' ? '631' : form.protocol === 'raw' ? '9100' : '515'" />
					</label>
					<label v-if="form.protocol !== 'raw'" class="field"><span>{{ form.protocol === "ipp" ? "Path" : "Queue" }}</span>
						<input v-model="form.queue" :placeholder="form.protocol === 'ipp' ? 'ipp/print' : 'queue'" />
					</label>
				</div>
			</fieldset>

			<label v-else class="field">
				<span>{{ isWindows ? "Existing port name (e.g. USB001, LPT1)" : "Device URI (e.g. socket://10.0.0.5:9100)" }}</span>
				<input v-model="form.manualUri" required />
			</label>

			<fieldset>
				<legend>Driver</legend>
				<label v-if="!driverRequired" class="row tight check">
					<input v-model="form.driverMode" type="radio" value="auto" /> Automatic (IPP Everywhere / driverless)
				</label>
				<label class="row tight check">
					<input v-model="form.driverMode" type="radio" value="pick" /> Choose an installed driver
				</label>
				<template v-if="form.driverMode === 'pick'">
					<AutocompleteField v-model="form.driver" :options="driverOptions" placeholder="Search drivers, e.g. Zebra, Epson…" aria-label="Printer driver" @search="driverQuery = $event" />
					<p v-if="!drivers.length" class="hint">No drivers found. Install the printer manufacturer's driver first.</p>
				</template>
				<p v-else class="hint">Suitable for modern network printers. USB label/receipt printers usually need the manufacturer's driver.</p>
			</fieldset>

			<label class="field">
				<span>Printer name {{ isWindows ? "" : "(no spaces)" }}</span>
				<input v-model="form.name" required maxlength="127" placeholder="e.g. Zebra_Counter" />
			</label>
			<div class="grid2">
				<label class="field"><span>Description</span><input v-model="form.description" maxlength="200" /></label>
				<label class="field"><span>Location</span><input v-model="form.location" maxlength="200" placeholder="e.g. Counter 1" /></label>
			</div>
			<label class="row tight check"><input v-model="form.shared" type="checkbox" /> Share on the network</label>
			</template>

			<p v-if="error" class="error">{{ error }}</p>
			<div class="row wrap">
				<span class="grow"></span>
				<button type="button" class="ghost" @click="emit('cancel')">Cancel</button>
				<button type="submit" class="primary" :disabled="busy || (editing && !details)">
					{{ busy ? "Processing…" : editing ? "Save" : "Add" }}
			</button>
		</div>
	</form>
</template>

<style scoped>
.segmented {
	display: flex;
	border: 1px solid var(--line);
	border-radius: 8px;
	overflow: hidden;
}
.segmented button {
	flex: 1;
	min-height: var(--control-height);
	border: none;
	background: var(--surface);
	color: var(--muted);
	padding: 7px 4px;
	font: inherit;
	font-size: 13px;
	cursor: pointer;
}
.segmented button.on {
	background: var(--brand-fill);
	color: #fff;
}
.printer-name-row {
	display: flex;
	align-items: flex-end;
	gap: 8px;
}
.action-menu {
	position: relative;
	flex: none;
}
.action-trigger {
	flex: none;
}
.action-popover {
	position: absolute;
	right: 0;
	top: calc(100% + 6px);
	z-index: 5;
	min-width: 210px;
	padding: 4px;
	background: var(--surface);
	border: 1px solid var(--line);
	border-radius: var(--r-md);
	box-shadow: 0 6px 18px rgb(0 0 0 / 0.14);
}
.menu-action {
	width: 100%;
	min-height: 34px;
	padding: 7px 8px;
	display: flex;
	align-items: center;
	gap: 8px;
	border: none;
	border-radius: var(--r-sm);
	background: transparent;
	color: var(--text);
	font: inherit;
	font-size: 13px;
	text-align: left;
	cursor: pointer;
}
.menu-action:hover:not(:disabled) {
	background: var(--brand-pale);
}
.menu-action.danger {
	color: var(--danger);
}
.menu-action.danger:hover:not(:disabled) {
	background: var(--danger-soft);
}
</style>
