<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { CircleCheck, ChevronLeft, ChevronRight, History as HistoryIcon, RefreshCw, Webhook } from "lucide-vue-next";
import EmptyState from "@/shared/ui/EmptyState.vue";
import AutocompleteField from "@/shared/ui/AutocompleteField.vue";
import {
  api,
  errorText,
  type Config,
  type ContentAvailability,
  type HistoryContentDetail,
  type HistoryDay,
  type HistoryEntry,
  type HistoryPdfPage,
  type JobStatus,
} from "@/shared/api";
import { CALLBACK_PENDING, describeCallback } from "@/shared/lib/callback";
import { describeJob, FINAL_STATES } from "@/shared/lib/job";

const props = defineProps<{ config: Config }>();
const emit = defineEmits<{
  (e: "config", config: Config): void;
  (e: "notify", message: string, error?: boolean): void;
}>();

const days = ref<HistoryDay[]>([]);
const date = ref("");
const entries = ref<HistoryEntry[]>([]);
const PER_PAGE = 20;
const page = ref(1);
const pages = ref(1);
const total = ref(0);
const failedOnly = ref(false);
const search = ref("");
const searching = computed(() => search.value.trim().length >= 3);
const expanded = ref<string | null>(null);
const detailTab = ref<"input" | "preview">("input");
const detailContent = ref<HistoryContentDetail | null>(null);
const detailLoadedFor = ref<string | null>(null);
const detailLoading = ref(false);
const detailError = ref("");
const pdfPage = ref<HistoryPdfPage | null>(null);
const pdfLoading = ref(false);
const pdfError = ref("");
const inputPdfPage = ref(1);
const previewPdfPage = ref(1);
const retention = ref(props.config.history_retention_days);
const isCancelled = (entry: HistoryEntry) => entry.source.startsWith("incoming_cancelled_");
const statusLabel = (entry: HistoryEntry) => (entry.success ? "Success" : isCancelled(entry) ? "Cancelled" : "Error");
const statusClass = (entry: HistoryEntry) => (entry.success ? "success" : isCancelled(entry) ? "cancelled" : "failure");
function sourceLabel(source: string) {
  const incomingSource = source.replace(/^incoming_(?:cancelled_)?/, "");
  const cancelled = source.startsWith("incoming_cancelled_");
  const label: Record<string, string> = {
    system_print: "system printer",
    file_picker: "file picker",
    finder: "Finder",
    explorer: "Explorer",
  };
  if (source.startsWith("incoming_")) {
    return `${cancelled ? "cancelled before printing · " : ""}${label[incomingSource] ?? "incoming document"}`;
  }
  return source === "app" ? "test print" : "API integration";
}
const dateOptions = computed(() => [
  ...(date.value && !days.value.some((d) => d.date === date.value)
    ? [{ value: date.value, label: `${date.value} (empty)` }]
    : []),
  ...days.value.map((d) => ({
    value: d.date,
    label: `${d.date} — ${d.total} job${d.failed ? `, ${d.failed} failed` : ""}`,
  })),
]);

function today() {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

let refreshSeq = 0;
let detailSeq = 0;
let pdfSeq = 0;
async function refresh() {
  const seq = ++refreshSeq;
  try {
    days.value = await api.historyDays();
    if (seq !== refreshSeq) return;
    if (!date.value) date.value = days.value[0]?.date ?? today();
    const result = searching.value
      ? await api.searchHistory(search.value.trim(), page.value, PER_PAGE)
      : await api.historyPage(date.value, page.value, PER_PAGE, failedOnly.value);
    if (seq !== refreshSeq) return;
    entries.value = result.entries;
    [page.value, pages.value, total.value] = [result.page, result.pages, result.total];
    pollStatuses();
  } catch (e) {
    if (seq === refreshSeq) emit("notify", errorText(e), true);
  }
}

/** Live spooler status of today's sent jobs, until each reaches a final state. */
const statuses = ref<Record<string, JobStatus>>({});
const MAX_TRACKED = 20;

async function pollStatuses() {
  // Callbacks still waiting or retrying: reload the page to pick up their progress.
  if (entries.value.some((e) => e.callback && CALLBACK_PENDING.includes(e.callback.state))) {
    const snapshot = [search.value, date.value, page.value, failedOnly.value];
    try {
      const result = (
        searching.value
          ? await api.searchHistory(search.value.trim(), page.value, PER_PAGE)
          : await api.historyPage(date.value, page.value, PER_PAGE, failedOnly.value)
      ).entries;
      if (snapshot.every((value, index) => value === [search.value, date.value, page.value, failedOnly.value][index])) {
        entries.value = result;
      }
    } catch {
      // next tick retries
    }
  }
  const pending = entries.value
    .filter((e) => e.time.startsWith(today()) && e.success && (e.spooler_job || e.remote))
    .slice(0, MAX_TRACKED)
    .filter((e) => !FINAL_STATES.includes(statuses.value[e.id]?.state ?? ""));
  for (const e of pending) {
    try {
      statuses.value[e.id] = await api.jobStatus(e.id);
    } catch {
      // history entry may be gone after pruning; ignore
    }
  }
}

// A different day or filter starts again at the first page.
watch([date, failedOnly], () => {
  page.value = 1;
  refresh();
});
let searchTimer: number | undefined;
watch(search, () => {
  window.clearTimeout(searchTimer);
  if (search.value.trim().length > 0 && !searching.value) return;
  searchTimer = window.setTimeout(() => {
    page.value = 1;
    refresh();
  }, 250);
});
function goTo(p: number) {
  page.value = Math.min(Math.max(1, p), pages.value);
  refresh();
  document.querySelector("main")?.scrollIntoView({ block: "start" });
}
onMounted(refresh);
const statusTimer = window.setInterval(pollStatuses, 3000);
onBeforeUnmount(() => {
  clearInterval(statusTimer);
  window.clearTimeout(searchTimer);
});

async function saveRetention() {
  try {
    emit("config", await api.setHistoryRetention(Number(retention.value)));
    emit("notify", `History retained for ${retention.value} days`);
    refresh();
  } catch (e) {
    emit("notify", errorText(e), true);
  }
}

async function retryCallback(jobId: string) {
  try {
    await api.retryCallback(jobId);
    emit("notify", "Callback retry started");
    refresh();
  } catch (error) {
    emit("notify", errorText(error), true);
  }
}

async function cancelJob(jobId: string) {
  try {
    await api.cancelOwnJob(jobId);
    emit("notify", "Cancellation requested from the printer queue");
    await refresh();
  } catch (error) {
    emit("notify", errorText(error), true);
  }
}

function clearDetailContent() {
  detailContent.value = null;
  detailLoadedFor.value = null;
  detailLoading.value = false;
  detailError.value = "";
  pdfPage.value = null;
  pdfLoading.value = false;
  pdfError.value = "";
  inputPdfPage.value = 1;
  previewPdfPage.value = 1;
  detailSeq += 1;
  pdfSeq += 1;
}

function toggleDetails(jobId: string) {
  const next = expanded.value === jobId ? null : jobId;
  expanded.value = next;
  detailTab.value = "input";
  clearDetailContent();
  if (next) void loadInput(next);
}

async function loadInput(jobId: string) {
  if (detailLoadedFor.value === jobId) {
    if (detailContent.value?.content?.input_kind === "pdf" && detailContent.value.input_availability === "available") {
      void loadPdfPage(jobId, "input", inputPdfPage.value);
    }
    return;
  }
  const seq = ++detailSeq;
  pdfSeq += 1;
  detailLoading.value = true;
  detailError.value = "";
  pdfPage.value = null;
  pdfError.value = "";
  try {
    const result = await api.historyContent(jobId);
    if (seq !== detailSeq || expanded.value !== jobId || detailTab.value !== "input") return;
    detailContent.value = result;
    detailLoadedFor.value = jobId;
    if (result.content?.input_kind === "pdf" && result.input_availability === "available") {
      void loadPdfPage(jobId, "input", inputPdfPage.value);
    }
  } catch (error) {
    if (seq === detailSeq && expanded.value === jobId && detailTab.value === "input") {
      detailError.value = errorText(error);
    }
  } finally {
    if (seq === detailSeq) detailLoading.value = false;
  }
}

async function loadPdfPage(jobId: string, document: "input" | "output", page: number) {
  const seq = ++pdfSeq;
  pdfLoading.value = true;
  pdfError.value = "";
  pdfPage.value = null;
  try {
    const result = await api.historyPdfPage(jobId, document, page);
    const isRelevant =
      expanded.value === jobId &&
      ((document === "input" && detailTab.value === "input") ||
        (document === "output" && detailTab.value === "preview"));
    if (seq === pdfSeq && isRelevant) pdfPage.value = result;
  } catch (error) {
    const isRelevant =
      expanded.value === jobId &&
      ((document === "input" && detailTab.value === "input") ||
        (document === "output" && detailTab.value === "preview"));
    if (seq === pdfSeq && isRelevant) pdfError.value = errorText(error);
  } finally {
    if (seq === pdfSeq) pdfLoading.value = false;
  }
}

function selectDetailTab(entry: HistoryEntry, tab: "input" | "preview") {
  detailTab.value = tab;
  pdfSeq += 1;
  pdfPage.value = null;
  pdfLoading.value = false;
  pdfError.value = "";
  if (tab === "input") {
    if (detailLoadedFor.value !== entry.id) void loadInput(entry.id);
    else if (
      detailContent.value?.content?.input_kind === "pdf" &&
      detailContent.value.input_availability === "available"
    ) {
      void loadPdfPage(entry.id, "input", inputPdfPage.value);
    }
  } else if (entry.content?.output === "saved") {
    void loadPdfPage(entry.id, "output", previewPdfPage.value);
  }
}

function movePdfPage(entry: HistoryEntry, document: "input" | "output", page: number) {
  if (page < 1) return;
  if (document === "input") inputPdfPage.value = page;
  else previewPdfPage.value = page;
  void loadPdfPage(entry.id, document, page);
}

function inputAvailabilityText(entry: HistoryEntry): string {
  switch (detailContent.value?.input_availability) {
    case "not_saved":
      return "Content was not saved for this job";
    case "no_request_body":
      return entry.source === "app" ? "No request body — local test print" : "No request body was received";
    case "save_failed":
      return "Input content could not be saved for this job";
    case "unavailable":
      return "Content is no longer available";
    default:
      return "";
  }
}

function previewAvailabilityText(entry: HistoryEntry): string {
  if (!entry.content) return "Content was not saved for this job";
  switch (entry.content.output) {
    case "not_created":
      return "Print preview is unavailable because a final PDF was not created";
    case "save_failed":
      return "Print preview could not be saved for this job";
    case "remote_unavailable":
      return "Print preview is unavailable on this computer";
    case "saved":
      return "";
  }
}

function pdfAvailabilityText(availability: ContentAvailability): string {
  switch (availability) {
    case "not_saved":
      return "Content was not saved for this job";
    case "no_request_body":
      return "No PDF was received for this job";
    case "not_pdf":
      return "The saved input is not a PDF";
    case "save_failed":
      return "PDF content could not be saved for this job";
    case "not_created":
      return "Print preview is unavailable because a final PDF was not created";
    case "remote_unavailable":
      return "Print preview is unavailable on this computer";
    case "unavailable":
      return "Content is no longer available";
    case "available":
      return "";
  }
}

const time = (iso: string) => iso.slice(11, 19);
</script>

<template>
  <section class="stack">
    <label class="field">
      <span>Search all history by integration token, client IP, job ID, reference, file, printer or preset</span>
      <input v-model="search" type="search" placeholder="Integration name or other detail" />
    </label>
    <div class="row">
      <AutocompleteField
        v-if="!searching"
        v-model="date"
        class="grow"
        :options="dateOptions"
        aria-label="History date"
      />
      <span v-else class="grow meta">Searching across all saved dates</span>
      <button class="icon" title="Refresh" aria-label="Refresh print history" @click="refresh">
        <RefreshCw :size="16" />
      </button>
    </div>

    <div class="row">
      <label v-if="!searching && (total || failedOnly)" class="row tight check grow">
        <input v-model="failedOnly" type="checkbox" /> Submission failed only
      </label>
      <span v-if="total" class="meta">
        {{ (page - 1) * PER_PAGE + 1 }}–{{ Math.min(page * PER_PAGE, total) }} dari {{ total }}
      </span>
    </div>

    <EmptyState
      v-if="!entries.length && searching"
      :icon="HistoryIcon"
      title="No matching jobs"
      text="Try an integration token, client IP, job ID, callback reference, filename, printer or preset."
    />
    <EmptyState
      v-else-if="!entries.length && failedOnly"
      :icon="CircleCheck"
      title="No failed jobs"
      text="All print jobs on this date were submitted successfully. Check each job status for its final outcome."
    >
      <button class="ghost small" @click="failedOnly = false">Show all</button>
    </EmptyState>
    <EmptyState
      v-else-if="!entries.length"
      :icon="HistoryIcon"
      title="No print jobs yet"
      :text="
        days.length
          ? 'No jobs on this date. Choose another date above.'
          : 'API integration prints and local test prints are recorded here, grouped by day.'
      "
    />

    <article v-for="e in entries" :key="e.id" class="card stack tight history-entry">
      <button
        type="button"
        class="history-entry-summary"
        :aria-expanded="expanded === e.id"
        :aria-label="`${expanded === e.id ? 'Hide' : 'Show'} details for ${e.file_name}`"
        @click="toggleDetails(e.id)"
      >
        <span class="row tight">
          <span :class="['dot', e.success || isCancelled(e) ? 'idle' : 'fail']" :title="statusLabel(e)"></span>
          <strong :class="['history-submit-status', statusClass(e)]">{{ statusLabel(e) }}</strong>
          <span class="mono">{{ time(e.time) }}</span>
          <strong class="grow truncate">{{ e.file_name }}</strong>
          <span class="meta">{{ e.sheets }} sheet{{ e.sheets === 1 ? "" : "s" }}</span>
          <ChevronRight
            :size="16"
            class="history-entry-caret"
            :class="{ expanded: expanded === e.id }"
            aria-hidden="true"
          />
        </span>
        <span class="meta truncate">
          {{ e.printer || "—" }}<template v-if="e.remote"> · via {{ e.remote.remote_name }}</template
          ><template v-if="e.preset"> · preset {{ e.preset }}</template> ·
          {{ sourceLabel(e.source) }}
          <template v-if="e.client"> · {{ e.client }}</template>
          <template v-if="e.token"> · integration {{ e.token }}</template>
        </span>
      </button>
      <div v-if="e.callback" :class="['row', 'tight', 'small-text', 'callback', e.callback.state]">
        <Webhook :size="14" />
        <span class="grow min0 truncate">{{ describeCallback(e.callback) }}</span>
      </div>
      <Transition name="detail">
        <div v-if="expanded === e.id" class="history-entry-details stack tight">
          <section class="history-detail-status stack tight" aria-label="Print status">
            <strong>Status</strong>
            <div class="row tight">
              <strong :class="['history-submit-status', statusClass(e)]">{{ statusLabel(e) }}</strong>
              <span v-if="statuses[e.id]" class="meta small-text"
                >Printer queue · {{ describeJob(statuses[e.id]) }}</span
              >
            </div>
            <p v-if="e.error" :class="isCancelled(e) ? 'meta small-text' : 'error small-text'">{{ e.error }}</p>
            <p v-else-if="e.success" class="meta small-text">
              Request submitted to the printer queue. This does not confirm that paper was physically printed.
            </p>
            <p v-else class="meta small-text">No error message was recorded for this job.</p>
            <div
              v-if="
                e.success && e.spooler_job && !e.remote && ['queued', 'paused'].includes(statuses[e.id]?.state ?? '')
              "
              class="row"
            >
              <button type="button" class="ghost small" @click.stop="cancelJob(e.id)">Cancel queued job</button>
            </div>
          </section>

          <div class="row wrap history-content-tabs" role="group" aria-label="Job content">
            <button
              type="button"
              class="ghost small"
              :aria-pressed="detailTab === 'input'"
              @click="selectDetailTab(e, 'input')"
            >
              Input
            </button>
            <button
              type="button"
              class="ghost small"
              :aria-pressed="detailTab === 'preview'"
              @click="selectDetailTab(e, 'preview')"
            >
              Print preview
            </button>
          </div>

          <section v-if="detailTab === 'input'" class="history-content-panel stack tight" aria-label="Print input">
            <p v-if="detailLoading" class="meta small-text">Loading saved input…</p>
            <p v-else-if="detailError" class="error small-text">Could not read saved content: {{ detailError }}</p>
            <template v-else-if="detailContent">
              <p v-if="detailContent.input_availability !== 'available'" class="meta small-text">
                {{ inputAvailabilityText(e) }}
              </p>
              <template
                v-else-if="
                  detailContent.content?.input_kind === 'json' || detailContent.content?.input_kind === 'sample'
                "
              >
                <p v-if="detailContent.content.input_kind === 'sample'" class="meta small-text">
                  Sample data used by this design test print
                </p>
                <pre class="settings history-json" tabindex="0" aria-label="Received JSON body">{{
                  JSON.stringify(detailContent.input_json, null, 2)
                }}</pre>
              </template>
              <template v-else-if="detailContent.content?.input_kind === 'pdf'">
                <p v-if="pdfLoading" class="meta small-text">Loading input PDF page…</p>
                <p v-else-if="pdfError" class="error small-text">Could not read saved PDF: {{ pdfError }}</p>
                <template v-else-if="pdfPage">
                  <p v-if="pdfPage.availability !== 'available'" class="meta small-text">
                    {{ pdfAvailabilityText(pdfPage.availability) }}
                  </p>
                  <template v-else>
                    <div class="row history-pdf-controls">
                      <button
                        type="button"
                        class="ghost small"
                        aria-label="Previous input PDF page"
                        :disabled="pdfPage.page <= 1"
                        @click.stop="movePdfPage(e, 'input', pdfPage.page - 1)"
                      >
                        <ChevronLeft :size="14" /> Previous
                      </button>
                      <span class="grow meta center">Page {{ pdfPage.page }} of {{ pdfPage.page_count }}</span>
                      <button
                        type="button"
                        class="ghost small"
                        aria-label="Next input PDF page"
                        :disabled="pdfPage.page >= (pdfPage.page_count ?? 1)"
                        @click.stop="movePdfPage(e, 'input', pdfPage.page + 1)"
                      >
                        Next <ChevronRight :size="14" />
                      </button>
                    </div>
                    <img
                      v-if="pdfPage.image_data_url"
                      class="history-pdf-image"
                      :src="pdfPage.image_data_url"
                      :alt="`Input PDF page ${pdfPage.page}`"
                    />
                  </template>
                </template>
              </template>
            </template>
          </section>

          <section v-else class="history-content-panel stack tight" aria-label="Print preview">
            <p v-if="previewAvailabilityText(e)" class="meta small-text">{{ previewAvailabilityText(e) }}</p>
            <p v-else-if="pdfLoading" class="meta small-text">Preparing print preview page…</p>
            <p v-else-if="pdfError" class="error small-text">Could not read print preview: {{ pdfError }}</p>
            <template v-else-if="pdfPage">
              <p v-if="pdfPage.availability !== 'available'" class="meta small-text">
                {{ pdfAvailabilityText(pdfPage.availability) }}
              </p>
              <template v-else>
                <div class="row history-pdf-controls">
                  <button
                    type="button"
                    class="ghost small"
                    aria-label="Previous print preview page"
                    :disabled="pdfPage.page <= 1"
                    @click.stop="movePdfPage(e, 'output', pdfPage.page - 1)"
                  >
                    <ChevronLeft :size="14" /> Previous
                  </button>
                  <span class="grow meta center">Page {{ pdfPage.page }} of {{ pdfPage.page_count }}</span>
                  <button
                    type="button"
                    class="ghost small"
                    aria-label="Next print preview page"
                    :disabled="pdfPage.page >= (pdfPage.page_count ?? 1)"
                    @click.stop="movePdfPage(e, 'output', pdfPage.page + 1)"
                  >
                    Next <ChevronRight :size="14" />
                  </button>
                </div>
                <img
                  v-if="pdfPage.image_data_url"
                  class="history-pdf-image"
                  :src="pdfPage.image_data_url"
                  :alt="`Print preview page ${pdfPage.page}`"
                />
              </template>
            </template>
          </section>

          <div v-if="e.callback" class="callback-detail small-text">
            <button
              v-if="e.callback.state === 'failed'"
              type="button"
              class="ghost small"
              @click.stop="retryCallback(e.id)"
            >
              Retry callback
            </button>
            <div>
              <span class="meta">URL</span> <code>{{ e.callback.url }}</code>
            </div>
            <div v-if="e.callback.reference">
              <span class="meta">Reference</span> <code>{{ e.callback.reference }}</code>
            </div>
            <div>
              <span class="meta">Attempts</span> {{ e.callback.attempts }}
              <template v-if="e.callback.http_status"> · HTTP {{ e.callback.http_status }}</template>
              · {{ time(e.callback.time) }}
            </div>
            <div v-if="e.callback.error && e.callback.state !== 'delivered'" class="error">{{ e.callback.error }}</div>
          </div>
          <div class="meta small-text">Resolved print settings</div>
          <pre class="settings">{{ JSON.stringify(e.settings, null, 2) }}</pre>
          <div v-if="e.remote" class="small-text">
            Job on {{ e.remote.remote_name }}: <code>{{ e.remote.job_id }}</code>
          </div>
        </div>
      </Transition>
    </article>

    <div v-if="pages > 1" class="row pager">
      <button class="ghost small" :disabled="page <= 1" @click="goTo(page - 1)">
        <ChevronLeft :size="14" /> Previous
      </button>
      <span class="grow meta center">Page {{ page }} / {{ pages }}</span>
      <button class="ghost small" :disabled="page >= pages" @click="goTo(page + 1)">
        Next <ChevronRight :size="14" />
      </button>
    </div>

    <fieldset>
      <legend>History settings</legend>
      <div class="row">
        <label class="field grow">
          <span>Keep history for (days)</span>
          <input v-model="retention" type="number" min="1" max="3650" />
          <span class="hint small-text">
            This period also applies to locally saved request bodies and PDFs. They use disk space and may contain
            sensitive data.
          </span>
        </label>
        <button class="ghost" @click="saveRetention">Save</button>
      </div>
    </fieldset>
  </section>
</template>

<style scoped>
.history-entry-summary {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
  min-height: var(--row-height);
  padding: 4px;
  border: 0;
  border-radius: var(--r-sm);
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.history-entry-summary:hover {
  background: var(--brand-pale);
}
.history-entry-summary:focus-visible {
  outline: 2px solid var(--accent);
}
.history-entry-caret {
  flex: none;
  color: var(--muted);
  transition: transform var(--motion-state-duration) var(--ease);
}
.history-entry-caret.expanded {
  transform: rotate(90deg);
}
.history-submit-status {
  flex: none;
}
.history-submit-status.success {
  color: var(--ok);
}
.history-submit-status.failure {
  color: var(--danger);
}
.history-submit-status.cancelled {
  color: var(--muted);
}
.history-detail-status p {
  margin: 0;
}
.history-content-tabs button[aria-pressed="true"] {
  border-color: var(--brand);
}
.history-content-panel {
  min-width: 0;
}
.history-json {
  max-height: 320px;
  overflow: auto;
  white-space: pre;
}
.history-pdf-controls {
  flex-wrap: wrap;
}
.history-pdf-image {
  display: block;
  max-width: 100%;
  max-height: 65vh;
  margin: 0 auto;
  object-fit: contain;
  border: 1px solid var(--line);
  background: #fff;
}
.callback {
  color: var(--muted);
}
.callback.delivered {
  color: var(--ok);
}
.callback.failed,
.callback.refused {
  color: var(--danger);
}
.callback.retrying {
  color: var(--warn);
}
.callback-detail {
  display: grid;
  gap: 2px;
  overflow-wrap: anywhere;
}
.pager .center {
  justify-content: center;
}
</style>
