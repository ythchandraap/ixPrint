<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { Copy, Pencil, Plus, Printer, RefreshCw, Star } from "lucide-vue-next";
import EmptyState from "./EmptyState.vue";
import PrinterDeviceForm from "./PrinterDeviceForm.vue";
import { api, describeJob, errorText, type Config, type PrinterQueue, type PrinterView } from "../api";

const props = defineProps<{
	printers: PrinterView[];
	config: Config | null;
	selected: string;
	refreshing: boolean;
	/** False until the first printer list arrived. */
	loaded: boolean;
	os: string;
}>();

const emit = defineEmits<{
	(e: "refresh"): void;
	(e: "select", name: string): void;
	(e: "toggle-default", name: string): void;
	(e: "copy", text: string): void;
	(e: "notify", message: string, error?: boolean): void;
	(e: "config", config: Config): void;
	/** Reload the list without a "N printers found" toast. */
	(e: "reload"): void;
}>();

/** Add/edit form for an OS printer; `name` undefined = add. */
const device = ref<{ name?: string } | null>(null);

function deviceDone(message: string, config?: Config) {
	device.value = null;
	if (config) emit("config", config);
	emit("notify", message);
	emit("reload");
}

/** Spooler queue of the selected printer, refreshed while it stays selected. */
const queue = ref<PrinterQueue | null>(null);
const queueError = ref("");
let queueTimer: number | undefined;

async function loadQueue() {
	const name = props.selected;
	if (!name) return;
	try {
		const q = await api.queue(name);
		if (props.selected === name) [queue.value, queueError.value] = [q, ""];
	} catch (e) {
		if (props.selected === name) [queue.value, queueError.value] = [null, errorText(e)];
	}
}

watch(
	() => props.selected,
	(name) => {
		clearInterval(queueTimer);
		queue.value = null;
		queueError.value = "";
		if (!name) return;
		loadQueue();
		queueTimer = window.setInterval(loadQueue, 3000);
	},
	{ immediate: true },
);
onBeforeUnmount(() => clearInterval(queueTimer));
</script>

<template>
	<PrinterDeviceForm
		v-if="device"
		:name="device.name"
		:os="os"
		:config="config!"
		@done="deviceDone"
		@cancel="device = null"
	/>
	<section v-else class="stack">
		<div class="row">
			<span class="grow meta">{{ printers.length }} printer{{ printers.length === 1 ? "" : "s" }} · updated automatically</span>
			<button class="ghost small" :disabled="refreshing" title="Find new printers now" @click="emit('refresh')">
				<RefreshCw :size="14" :class="{ spin: refreshing }" /> Refresh
			</button>
			<button class="ghost small" title="Add a printer to the system" @click="device = {}"><Plus :size="14" /> Add</button>
		</div>
		<p v-if="!loaded" class="muted">Finding printers…</p>
		<EmptyState
			v-else-if="!printers.length"
			:icon="Printer"
			title="No printers detected"
			text="Install a printer in your operating system's printer settings (driver, USB, or network). ix.print checks again every 3 seconds."
		>
			<button class="primary small" @click="device = {}"><Plus :size="14" /> Add printer</button>
			<button class="ghost small" :disabled="refreshing" @click="emit('refresh')">Search again</button>
		</EmptyState>
		<p v-else-if="!config?.default_printer" class="hint">
			No ix.print default printer is set, so the system default printer is used. Click ☆ to choose one.
		</p>

		<label
			v-for="printer in printers"
			:key="printer.name"
			class="row card"
			:class="{ active: selected === printer.name }"
		>
			<input
				type="radio"
				name="printer"
				:checked="selected === printer.name"
				@change="emit('select', printer.name)"
			/>
			<div class="grow min0">
				<div class="truncate strong">{{ printer.name }}</div>
				<div class="meta">
					<span :class="['status', printer.status.toLowerCase()]">{{ printer.status }}</span>
					<span v-if="printer.is_default" class="badge">default ix.print</span>
					<span v-if="printer.os_default" class="badge subtle">system default</span>
					<span v-if="printer.remote" class="badge subtle">via {{ printer.remote }}</span>
				</div>
				<div v-if="printer.message" :class="printer.status === 'Normal' || printer.status === 'Printing' ? 'hint' : 'error small-text'">
					{{ printer.message }}
				</div>
			</div>
			<button
				class="icon"
				:title="config?.default_printer === printer.name ? 'Remove as default' : 'Set as default printer'"
				@click.prevent="emit('toggle-default', printer.name)"
			>
				<Star :size="16" :fill="config?.default_printer === printer.name ? 'currentColor' : 'none'" />
			</button>
			<button class="icon" title="Copy printer name" @click.prevent="emit('copy', printer.name)">
				<Copy :size="16" />
			</button>
			<button v-if="!printer.remote" class="icon" title="Edit / remove printer" @click.prevent="device = { name: printer.name }">
				<Pencil :size="16" />
			</button>
		</label>

		<fieldset v-if="selected">
			<legend>Queue for {{ selected }}</legend>
			<p v-if="queueError" class="error">{{ queueError }}</p>
			<template v-else-if="queue">
				<p v-if="queue.message" :class="queue.status === 'Normal' || queue.status === 'Printing' ? 'hint' : 'error'">
					⚠ {{ queue.message }}
				</p>
				<p v-if="!queue.jobs.length" class="muted">Queue is empty.</p>
				<div v-for="job in queue.jobs" :key="job.id" class="row tight queue-row">
					<span class="mono pos">#{{ job.position }}</span>
					<div class="grow min0">
						<div class="truncate">{{ job.document || job.id }}</div>
						<div :class="['meta', { 'error-text': job.state === 'error' }]">
							{{ describeJob({ ...job, ahead: job.position - 1 }) }}
							<template v-if="job.pages"> · {{ job.pages_printed ?? 0 }}/{{ job.pages }} pages</template>
						</div>
					</div>
				</div>
			</template>
			<p v-else class="muted">Loading queue…</p>
		</fieldset>
		<p v-else-if="printers.length" class="hint">Select a printer to view its queue.</p>
	</section>
</template>

<style scoped>
.queue-row {
	align-items: flex-start;
	padding: 4px 0;
	border-top: 1px solid var(--line);
}
.pos {
	min-width: 28px;
	padding-top: 2px;
}
.error-text {
	color: var(--danger);
}
.spin {
	animation: spin 0.8s linear infinite;
}
@keyframes spin {
	to {
		transform: rotate(360deg);
	}
}
</style>
