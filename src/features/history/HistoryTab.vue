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

async function refresh() {
  try {
    days.value = await api.historyDays();
    if (!date.value) date.value = days.value[0]?.date ?? today();
    const result = await api.historyPage(date.value, page.value, PER_PAGE, failedOnly.value);
    entries.value = result.entries;
    [page.value, pages.value, total.value] = [result.page, result.pages, result.total];
    pollStatuses();
  } catch (e) {
    emit("notify", errorText(e), true);
  }
}

/** Live spooler status of today's sent jobs, until each reaches a final state. */
const statuses = ref<Record<string, JobStatus>>({});
const MAX_TRACKED = 20;

async function pollStatuses() {
  // Callbacks still waiting or retrying: reload the page to pick up their progress.
  if (entries.value.some((e) => e.callback && CALLBACK_PENDING.includes(e.callback.state))) {
    try {
      entries.value = (await api.historyPage(date.value, page.value, PER_PAGE, failedOnly.value)).entries;
    } catch {
      // next tick retries
    }
  }
  if (date.value !== today()) return;
  const pending = entries.value
    .filter((e) => e.success && (e.spooler_job || e.remote))
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
function goTo(p: number) {
  page.value = Math.min(Math.max(1, p), pages.value);
  refresh();
  document.querySelector("main")?.scrollIntoView({ block: "start" });
}
onMounted(refresh);
const statusTimer = window.setInterval(pollStatuses, 3000);
onBeforeUnmount(() => clearInterval(statusTimer));

async function saveRetention() {
  try {
    emit("config", await api.setHistoryRetention(Number(retention.value)));
    emit("notify", `History retained for ${retention.value} days`);
    refresh();
  } catch (e) {
    emit("notify", errorText(e), true);
  }
}

const time = (iso: string) => iso.slice(11, 19);
</script>

<template>
  <section class="stack">
    <div class="row">
      <AutocompleteField v-model="date" class="grow" :options="dateOptions" aria-label="History date" />
      <button class="icon" title="Refresh" @click="refresh"><RefreshCw :size="16" /></button>
    </div>

    <div class="row">
      <label v-if="total || failedOnly" class="row tight check grow">
        <input v-model="failedOnly" type="checkbox" /> Failed only
      </label>
      <span v-if="total" class="meta">
        {{ (page - 1) * PER_PAGE + 1 }}–{{ Math.min(page * PER_PAGE, total) }} dari {{ total }}
      </span>
    </div>

    <EmptyState
      v-if="!entries.length && failedOnly"
      :icon="CircleCheck"
      title="No failed jobs"
      text="All print jobs on this date were sent successfully."
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
          : 'Every print from a web app or test print is recorded here, grouped by day.'
      "
    />

    <div
      v-for="e in entries"
      :key="e.id"
      class="card stack tight clickable"
      @click="expanded = expanded === e.id ? null : e.id"
    >
      <div class="row tight">
        <span :class="['dot', e.success ? 'ok' : 'fail']"></span>
        <span class="mono">{{ time(e.time) }}</span>
        <strong class="grow truncate">{{ e.file_name }}</strong>
        <span class="meta">{{ e.sheets }} sheet{{ e.sheets === 1 ? "" : "s" }}</span>
      </div>
      <div class="meta truncate">
        {{ e.printer || "—" }}<template v-if="e.remote"> · via {{ e.remote.remote_name }}</template
        ><template v-if="e.preset"> · preset {{ e.preset }}</template> ·
        {{ e.source === "app" ? "test print" : (e.client ?? "web")
        }}<template v-if="e.token"> · token {{ e.token }}</template>
      </div>
      <div v-if="e.error" class="error small-text">{{ e.error }}</div>
      <div v-else-if="statuses[e.id]" :class="['small-text', statuses[e.id].state === 'error' ? 'error' : 'meta']">
        {{ describeJob(statuses[e.id]) }}
      </div>
      <div v-if="e.callback" :class="['row', 'tight', 'small-text', 'callback', e.callback.state]">
        <Webhook :size="14" />
        <span class="grow min0 truncate">{{ describeCallback(e.callback) }}</span>
      </div>
      <div v-if="expanded === e.id && e.callback" class="callback-detail small-text">
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
      <pre v-if="expanded === e.id" class="settings">{{ JSON.stringify(e.settings, null, 2) }}</pre>
      <div v-if="expanded === e.id && e.remote" class="small-text">
        Job on {{ e.remote.remote_name }}: <code>{{ e.remote.job_id }}</code>
      </div>
    </div>

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
