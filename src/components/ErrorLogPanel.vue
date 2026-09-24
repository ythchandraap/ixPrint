<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { ChevronLeft, ChevronRight, Copy, ExternalLink, RefreshCw, ShieldAlert, Trash2, X } from "lucide-vue-next";
import { api, errorText, type DiagnosticEntry } from "../api";

const props = defineProps<{ showSystemLogs: boolean }>();
const emit = defineEmits<{
	(e: "copy", text: string): void;
	(e: "notify", message: string, error?: boolean): void;
}>();

const entries = ref<DiagnosticEntry[]>([]);
const loading = ref(false);
const filterText = ref("");
const currentPage = ref(1);
const selectedEntry = ref<DiagnosticEntry | null>(null);
const detailDialog = ref<HTMLDialogElement | null>(null);
const PAGE_SIZE = 20;
let previousBodyOverflow: string | null = null;

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

function formatEntry(entry: DiagnosticEntry) {
	const details = entry.details ? `\n${entry.details}` : "";
	return `${entry.time} [${entry.level}] ${entry.source}\n${entry.message}${details}`;
}

function copyLog() {
	emit("copy", entries.value.map(formatEntry).join("\n\n"));
}

function copyEntry(entry: DiagnosticEntry) {
	emit("copy", formatEntry(entry));
}

async function clear() {
	if (!(await confirm("Clear all locally saved error logs?", { title: "Clear error log", kind: "warning" }))) return;
	try {
		await api.clearErrorLog();
		entries.value = [];
		currentPage.value = 1;
		filterText.value = "";
		closeDetail();
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

const displayTime = (time: string) => {
	const date = new Date(time);
	return Number.isNaN(date.getTime()) ? time : date.toLocaleString();
};

const newestFirst = computed(() => [...entries.value].reverse());
const filteredEntries = computed(() => {
	const query = filterText.value.trim().toLowerCase();
	if (!query) return newestFirst.value;
	return newestFirst.value.filter((entry) =>
		[entry.level, entry.source, entry.time, displayTime(entry.time), entry.message, entry.details ?? ""].some((value) => value.toLowerCase().includes(query)),
	);
});
const pageCount = computed(() => Math.ceil(filteredEntries.value.length / PAGE_SIZE));
const visibleEntries = computed(() => filteredEntries.value.slice((currentPage.value - 1) * PAGE_SIZE, currentPage.value * PAGE_SIZE));
const rangeStart = computed(() => (filteredEntries.value.length ? (currentPage.value - 1) * PAGE_SIZE + 1 : 0));
const rangeEnd = computed(() => Math.min(currentPage.value * PAGE_SIZE, filteredEntries.value.length));

watch(filterText, () => (currentPage.value = 1));
watch(pageCount, (count) => {
	currentPage.value = Math.min(currentPage.value, Math.max(1, count));
});
watch(selectedEntry, async (entry) => {
	if (!entry) {
		if (previousBodyOverflow !== null) {
			document.body.style.overflow = previousBodyOverflow;
			previousBodyOverflow = null;
		}
		return;
	}
	if (previousBodyOverflow === null) {
		previousBodyOverflow = document.body.style.overflow;
		document.body.style.overflow = "hidden";
	}
	await nextTick();
	if (detailDialog.value && !detailDialog.value.open) detailDialog.value.showModal();
});

function closeDetail() {
	if (detailDialog.value?.open) detailDialog.value.close();
	else selectedEntry.value = null;
}

function closeFromBackdrop(event: MouseEvent) {
	if (event.target === detailDialog.value) closeDetail();
}

onMounted(refresh);
onBeforeUnmount(() => {
	if (previousBodyOverflow !== null) document.body.style.overflow = previousBodyOverflow;
});
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
			<template v-if="entries.length">
				<div class="log-list-tools">
					<label class="field" for="error-log-filter">
						<span>Filter logs</span>
						<input id="error-log-filter" v-model="filterText" type="search" placeholder="Search message, source, or level" />
					</label>
					<span class="meta log-count" aria-live="polite">{{ rangeStart }}–{{ rangeEnd }} of {{ filteredEntries.length }} logs</span>
				</div>

				<div id="error-log-list" class="error-entry-list">
					<button
						v-for="(entry, index) in visibleEntries"
						:key="`${entry.time}-${index}-${entry.source}`"
						class="error-entry"
						type="button"
						:aria-label="`Open log: ${entry.message}; ${entry.level} from ${entry.source} at ${displayTime(entry.time)}`"
						aria-haspopup="dialog"
						@click="selectedEntry = entry"
					>
						<span class="error-entry-topline">
							<strong :class="['error-entry-level', entry.level.toLowerCase()]">{{ entry.level.toUpperCase() }}</strong>
							<span class="meta">{{ entry.source }} · {{ displayTime(entry.time) }}</span>
							<ChevronRight class="entry-chevron" :size="15" />
						</span>
						<span class="error-entry-message">{{ entry.message }}</span>
					</button>
					<p v-if="!filteredEntries.length" class="muted no-matches">No logs match this filter.</p>
				</div>

				<nav v-if="pageCount > 1" class="log-pagination" aria-label="Error log pages">
					<button class="ghost small" :disabled="currentPage <= 1" @click="currentPage--"><ChevronLeft :size="14" /> Previous</button>
					<span class="meta" aria-live="polite">Page {{ currentPage }} of {{ pageCount }}</span>
					<button class="ghost small" :disabled="currentPage >= pageCount" @click="currentPage++">Next <ChevronRight :size="14" /></button>
				</nav>
			</template>
		</div>

		<dialog
			v-if="selectedEntry"
			ref="detailDialog"
			class="error-detail-dialog"
			aria-modal="true"
			aria-labelledby="error-detail-title"
			aria-describedby="error-detail-message"
			@click="closeFromBackdrop"
			@close="selectedEntry = null"
		>
			<header class="detail-header">
				<div>
					<p class="eyebrow">{{ selectedEntry.level.toUpperCase() }} · {{ selectedEntry.source }}</p>
					<h3 id="error-detail-title">Error log details</h3>
					<p class="meta">{{ displayTime(selectedEntry.time) }}</p>
				</div>
				<button class="icon" type="button" aria-label="Close log details" @click="closeDetail"><X :size="17" /></button>
			</header>
			<div class="detail-content">
				<section class="detail-section">
					<h4>Message</h4>
					<p id="error-detail-message" class="detail-message">{{ selectedEntry.message }}</p>
				</section>
				<section v-if="selectedEntry.details" class="detail-section">
					<h4>Technical details</h4>
					<pre>{{ selectedEntry.details }}</pre>
				</section>
				<p v-else class="muted">No technical details were recorded for this log.</p>
			</div>
			<footer class="detail-footer">
				<button class="ghost small" type="button" @click="copyEntry(selectedEntry)"><Copy :size="14" /> Copy this log</button>
				<button class="primary small" type="button" @click="closeDetail">Done</button>
			</footer>
		</dialog>
	</section>
</template>

<style scoped>
.error-logs { padding-top: 4px; }
.settings-group-heading { display: flex; align-items: flex-start; gap: 12px; padding: 0 2px; }
.settings-group-title { display: flex; align-items: flex-start; gap: 9px; color: var(--brand); }
.settings-group-title h2 { margin: 0; font-size: 16px; font-weight: 600; letter-spacing: -0.015em; color: var(--text); }
.settings-group-title p { margin: 2px 0 0; font-size: 12px; line-height: 1.45; color: var(--muted); }
.log-list-tools { display: flex; align-items: flex-end; justify-content: space-between; gap: 12px; padding-top: 4px; }
.log-list-tools .field { flex: 1; max-width: 420px; }
.log-count { padding-bottom: 8px; white-space: nowrap; }
.error-entry-list { border-top: 1px solid var(--line); }
.error-entry { display: block; width: 100%; padding: 11px 4px; border: 0; border-bottom: 1px solid var(--line); background: transparent; color: var(--text); text-align: left; cursor: pointer; }
.error-entry:hover { background: var(--brand-pale); }
.error-entry:focus-visible { outline: 2px solid var(--brand); outline-offset: -2px; }
.error-entry-topline { display: flex; align-items: center; gap: 8px; min-width: 0; }
.error-entry-level { flex: none; font-size: 10px; letter-spacing: 0.06em; color: var(--danger, #b42318); }
.error-entry-level.warn, .error-entry-level.warning { color: var(--warn); }
.error-entry-topline .meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.entry-chevron { flex: none; margin-left: auto; color: var(--muted); }
.error-entry-message { display: -webkit-box; overflow: hidden; margin-top: 5px; font-size: 13px; line-height: 1.45; overflow-wrap: anywhere; white-space: pre-wrap; -webkit-box-orient: vertical; -webkit-line-clamp: 2; }
.no-matches { padding: 10px 0; }
.log-pagination { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding-top: 4px; }
.error-detail-dialog { width: min(640px, calc(100vw - 32px)); max-width: none; max-height: min(80vh, 760px); margin: auto; padding: 0; overflow: hidden; border: 1px solid var(--line); border-radius: var(--r-lg); background: var(--surface); color: var(--text); box-shadow: 0 18px 60px rgb(0 0 0 / 0.28); }
.error-detail-dialog::backdrop { background: rgb(0 0 0 / 0.64); backdrop-filter: blur(2px); }
.detail-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; padding: 18px 20px 12px; border-bottom: 1px solid var(--line); }
.detail-header .eyebrow { margin: 0 0 4px; color: var(--danger); font-family: var(--font-mono); font-size: 10px; letter-spacing: 0.06em; }
.detail-header h3 { margin: 0; font-size: 17px; }
.detail-header .meta { margin: 4px 0 0; }
.detail-content { max-height: calc(80vh - 145px); overflow: auto; padding: 16px 20px; }
.detail-section + .detail-section { margin-top: 18px; }
.detail-section h4 { margin: 0 0 6px; font-size: 12px; }
.detail-message { margin: 0; overflow-wrap: anywhere; white-space: pre-wrap; }
.detail-content pre { max-height: 45vh; margin: 0; overflow: auto; padding: 12px; border: 1px solid var(--line); border-radius: var(--r-md); background: var(--bg); color: var(--text); font-family: var(--font-mono); font-size: 11px; line-height: 1.5; overflow-wrap: anywhere; white-space: pre-wrap; }
.detail-footer { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 20px; border-top: 1px solid var(--line); }
@media (max-width: 520px) {
	.log-list-tools { align-items: stretch; flex-direction: column; gap: 4px; }
	.log-count { padding: 0; }
	.error-entry-topline { flex-wrap: wrap; }
	.detail-header, .detail-content { padding-right: 14px; padding-left: 14px; }
	.detail-footer { padding-right: 14px; padding-left: 14px; }
}
</style>
