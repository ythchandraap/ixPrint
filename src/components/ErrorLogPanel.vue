<script setup lang="ts">
import { onMounted, ref } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { Copy, ExternalLink, RefreshCw, ShieldAlert, Trash2 } from "lucide-vue-next";
import { api, errorText, type DiagnosticEntry } from "../api";

const props = defineProps<{ showSystemLogs: boolean }>();
const emit = defineEmits<{
	(e: "copy", text: string): void;
	(e: "notify", message: string, error?: boolean): void;
}>();

const entries = ref<DiagnosticEntry[]>([]);
const loading = ref(false);

async function refresh() {
	loading.value = true;
	try {
		entries.value = await api.errorLog();
	} catch (e) {
		emit("notify", errorText(e), true);
	} finally {
		loading.value = false;
	}
}

function copyLog() {
	const text = entries.value
		.map((entry) => {
			const details = entry.details ? `\n${entry.details}` : "";
			return `${entry.time} [${entry.level}] ${entry.source}\n${entry.message}${details}`;
		})
		.join("\n\n");
	emit("copy", text);
}

async function clear() {
	if (!(await confirm("Clear all locally saved error logs?", { title: "Clear error log", kind: "warning" }))) return;
	try {
		await api.clearErrorLog();
		entries.value = [];
		emit("notify", "Local error log cleared");
	} catch (e) {
		emit("notify", errorText(e), true);
	}
}

async function openSystemLogs() {
	try {
		await api.openSystemLogs();
	} catch (e) {
		emit("notify", errorText(e), true);
	}
}

const displayTime = (time: string) => new Date(time).toLocaleString();
onMounted(refresh);
</script>

<template>
	<section id="error-logs" class="settings-group error-logs">
		<div class="settings-group-heading">
			<div class="settings-group-title">
				<ShieldAlert :size="18" />
				<div><h2>Error log</h2><p>Local diagnostics for app errors and abnormal shutdowns.</p></div>
			</div>
	</div>
		<div class="card stack tight">
			<p class="hint">ix.print logs stay on this computer. App errors and interrupted print jobs appear here; Windows driver and system faults are recorded separately by Windows.</p>
			<div class="row wrap">
				<button class="ghost small" :disabled="loading" @click="refresh"><RefreshCw :size="14" /> {{ loading ? "Loading…" : "Refresh" }}</button>
				<button class="ghost small" :disabled="!entries.length" @click="copyLog"><Copy :size="14" /> Copy logs to send</button>
				<button v-if="props.showSystemLogs" class="ghost small" @click="openSystemLogs"><ExternalLink :size="14" /> Open Windows Event Viewer</button>
				<button class="ghost small danger" :disabled="!entries.length" @click="clear"><Trash2 :size="14" /> Clear logs</button>
			</div>
			<p v-if="!entries.length && !loading" class="muted">No error logs recorded.</p>
			<div v-for="(entry, index) in [...entries].reverse()" :key="`${entry.time}-${index}`" class="error-entry">
				<div class="row tight error-entry-meta">
					<strong>{{ entry.level.toUpperCase() }}</strong>
					<span class="meta">{{ entry.source }} · {{ displayTime(entry.time) }}</span>
				</div>
				<p class="error-entry-message">{{ entry.message }}</p>
				<details v-if="entry.details">
					<summary>Technical details</summary>
					<pre>{{ entry.details }}</pre>
				</details>
			</div>
		</div>
	</section>
</template>

<style scoped>
.error-logs {
	padding-top: 4px;
}

.settings-group-heading {
	display: flex;
	align-items: flex-start;
	gap: 12px;
	padding: 0 2px;
}

.settings-group-title {
	display: flex;
	align-items: flex-start;
	gap: 9px;
	color: var(--brand);
}

.settings-group-title h2 {
	margin: 0;
	font-size: 16px;
	font-weight: 600;
	letter-spacing: -0.015em;
	color: var(--text);
}

.settings-group-title p {
	margin: 2px 0 0;
	font-size: 12px;
	line-height: 1.45;
	color: var(--muted);
}

.error-entry {
	padding: 10px 0;
	border-top: 1px solid var(--line);
}

.error-entry-meta strong {
	font-size: 10px;
	letter-spacing: 0.06em;
	color: var(--danger, #b42318);
}

.error-entry-message {
	margin: 6px 0;
	font-size: 13px;
	white-space: pre-wrap;
	overflow-wrap: anywhere;
}

details summary {
	cursor: pointer;
	font-size: 11px;
	color: var(--muted);
}

pre {
	max-height: 180px;
	margin: 8px 0 0;
	overflow: auto;
	white-space: pre-wrap;
	word-break: break-word;
	font-size: 10px;
	color: var(--muted);
}
</style>
