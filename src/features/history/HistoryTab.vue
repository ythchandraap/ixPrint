<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { CircleCheck, ChevronLeft, ChevronRight, History as HistoryIcon, RefreshCw, Webhook } from "lucide-vue-next";
import EmptyState from "@/shared/ui/EmptyState.vue";
import AutocompleteField from "@/shared/ui/AutocompleteField.vue";
import { api, errorText, type Config, type HistoryDay, type HistoryEntry, type JobStatus } from "@/shared/api";
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
const retention = ref(props.config.history_retention_days);
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
      const result = (searching.value
        ? await api.searchHistory(search.value.trim(), page.value, PER_PAGE)
        : await api.historyPage(date.value, page.value, PER_PAGE, failedOnly.value)).entries;
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
  searchTimer = window.setTimeout(() => { page.value = 1; refresh(); }, 250);
});
function goTo(p: number) {
  page.value = Math.min(Math.max(1, p), pages.value);
  refresh();
  document.querySelector("main")?.scrollIntoView({ block: "start" });
}
onMounted(refresh);
const statusTimer = window.setInterval(pollStatuses, 3000);
onBeforeUnmount(() => { clearInterval(statusTimer); window.clearTimeout(searchTimer); });

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

const time = (iso: string) => iso.slice(11, 19);
</script>

<template>
  <section class="stack">
    <label class="field">
      <span>Search all history by integration token, client IP, job ID, reference, file, printer or preset</span>
      <input v-model="search" type="search" placeholder="Integration name or other detail" />
    </label>
    <div class="row">
      <AutocompleteField v-if="!searching" v-model="date" class="grow" :options="dateOptions" aria-label="History date" />
      <span v-else class="grow meta">Searching across all saved dates</span>
      <button class="icon" title="Refresh" aria-label="Refresh print history" @click="refresh"><RefreshCw :size="16" /></button>
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
        @click="expanded = expanded === e.id ? null : e.id"
      >
        <span class="row tight">
          <span :class="['dot', e.success ? 'idle' : 'fail']" :title="e.success ? 'Submitted to printer' : 'Submission failed'"></span>
          <span class="mono">{{ time(e.time) }}</span>
          <strong class="grow truncate">{{ e.file_name }}</strong>
          <span class="meta">{{ e.sheets }} sheet{{ e.sheets === 1 ? "" : "s" }}</span>
          <ChevronRight :size="16" class="history-entry-caret" :class="{ expanded: expanded === e.id }" aria-hidden="true" />
        </span>
        <span class="meta truncate">
          {{ e.printer || "—" }}<template v-if="e.remote"> · via {{ e.remote.remote_name }}</template
          ><template v-if="e.preset"> · preset {{ e.preset }}</template> ·
          {{ e.source === "app" ? "test print" : "API integration" }}
          <template v-if="e.client"> · {{ e.client }}</template>
          <template v-if="e.token"> · integration {{ e.token }}</template>
        </span>
      </button>
      <div v-if="e.error" class="error small-text">Submission failed: {{ e.error }}</div>
      <div v-else-if="statuses[e.id]" :class="['small-text', statuses[e.id].state === 'error' ? 'error' : 'meta']">
        {{ describeJob(statuses[e.id]) }}
      </div>
      <div v-else-if="e.success" class="meta small-text">Submitted to printer · final outcome not recorded here</div>
      <div v-if="e.callback" :class="['row', 'tight', 'small-text', 'callback', e.callback.state]">
        <Webhook :size="14" />
        <span class="grow min0 truncate">{{ describeCallback(e.callback) }}</span>
      </div>
      <Transition name="detail">
        <div v-if="expanded === e.id" class="history-entry-details stack tight">
          <div v-if="e.callback" class="callback-detail small-text">
            <button v-if="e.callback.state === 'failed'" type="button" class="ghost small" @click.stop="retryCallback(e.id)">Retry callback</button>
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
          <div v-if="e.success && e.spooler_job && !e.remote && ['queued', 'paused'].includes(statuses[e.id]?.state ?? '')" class="row">
            <button type="button" class="ghost small" @click.stop="cancelJob(e.id)">Cancel queued job</button>
          </div>
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
