<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { ChevronLeft, ChevronRight, Network, RefreshCw, Trash2, Copy } from "lucide-vue-next";
import { api, errorText, type ApiAccessEntry } from "@/shared/api";

const emit = defineEmits<{
  (e: "copy", text: string): void;
  (e: "notify", message: string, error?: boolean): void;
}>();

const entries = ref<ApiAccessEntry[]>([]);
const loading = ref(false);
const filterText = ref("");
const currentPage = ref(1);
const PAGE_SIZE = 20;

async function refresh() {
  loading.value = true;
  try {
    entries.value = await api.apiAccessLog();
  } catch (error) {
    emit("notify", errorText(error), true);
  } finally {
    loading.value = false;
  }
}

async function clear() {
  if (!(await confirm("Clear all saved API access logs?", { title: "Clear API access log", kind: "warning" }))) return;
  try {
    await api.clearApiAccessLog();
    entries.value = [];
    currentPage.value = 1;
    filterText.value = "";
    emit("notify", "API access log cleared");
  } catch (error) {
    emit("notify", errorText(error), true);
  }
}

function copyLog() {
  emit("copy", JSON.stringify([...entries.value].reverse(), null, 2));
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
    [entry.method, entry.path, entry.client ?? "", entry.auth, entry.token ?? "", entry.origin ?? "", entry.user_agent ?? "", String(entry.response_status)]
      .some((value) => value.toLowerCase().includes(query)),
  );
});
const pageCount = computed(() => Math.ceil(filteredEntries.value.length / PAGE_SIZE));
const visibleEntries = computed(() =>
  filteredEntries.value.slice((currentPage.value - 1) * PAGE_SIZE, currentPage.value * PAGE_SIZE),
);
const rangeStart = computed(() => (filteredEntries.value.length ? (currentPage.value - 1) * PAGE_SIZE + 1 : 0));
const rangeEnd = computed(() => Math.min(currentPage.value * PAGE_SIZE, filteredEntries.value.length));

watch(filterText, () => (currentPage.value = 1));
watch(pageCount, (count) => (currentPage.value = Math.min(currentPage.value, Math.max(1, count))));
onMounted(refresh);
</script>

<template>
  <section class="settings-group api-access-log">
    <div class="settings-group-heading">
      <div class="settings-group-title">
        <Network :size="18" />
        <div>
          <h2>API access log</h2>
          <p>Request and response activity for the local HTTP API.</p>
        </div>
      </div>
    </div>
    <div class="card stack tight">
      <p class="hint">
        Records method, path, client, token name, response status and timing. Authorization values, query strings, and
        request/response bodies are not stored. The local log rotates at a size limit; this view shows the latest 3,000 requests.
      </p>
      <div class="row wrap">
        <button class="ghost small" :disabled="loading" @click="refresh">
          <RefreshCw :size="14" /> {{ loading ? "Loading…" : "Refresh" }}
        </button>
        <button class="ghost small" :disabled="!entries.length" @click="copyLog">
          <Copy :size="14" /> Copy log
        </button>
        <button class="ghost small danger" :disabled="!entries.length" @click="clear">
          <Trash2 :size="14" /> Clear log
        </button>
      </div>

      <p v-if="!entries.length && !loading" class="muted">No API requests recorded yet.</p>
      <template v-if="entries.length">
        <div class="log-list-tools">
          <label class="field" for="api-access-log-filter">
            <span>Filter requests</span>
            <input
              id="api-access-log-filter"
              v-model="filterText"
              type="search"
              placeholder="Path, token name, client, or status"
            />
          </label>
          <span class="meta log-count" aria-live="polite">{{ rangeStart }}–{{ rangeEnd }} of {{ filteredEntries.length }} requests</span>
        </div>

        <div class="api-access-entry-list">
          <details v-for="entry in visibleEntries" :key="entry.id" class="api-access-entry">
            <summary>
              <span :class="['api-method', entry.method.toLowerCase()]">{{ entry.method }}</span>
              <code class="api-path">{{ entry.path }}</code>
              <strong :class="['api-status', entry.response_status >= 500 ? 'bad' : entry.response_status >= 400 ? 'warn' : 'ok']">
                {{ entry.response_status }}
              </strong>
              <span class="meta">{{ entry.duration_ms }} ms</span>
              <span class="meta api-time">{{ displayTime(entry.time) }}</span>
              <ChevronRight :size="14" class="api-access-chevron" aria-hidden="true" />
            </summary>
            <dl class="api-access-details">
              <div><dt>Client</dt><dd>{{ entry.client ?? "unknown" }}</dd></div>
              <div><dt>Credential</dt><dd>{{ entry.auth }}<template v-if="entry.token"> · {{ entry.token }}</template></dd></div>
              <div><dt>Request</dt><dd>{{ entry.request_content_type ?? "unknown type" }}<template v-if="entry.request_bytes !== undefined"> · {{ entry.request_bytes }} bytes</template></dd></div>
              <div><dt>Response</dt><dd>{{ entry.response_content_type ?? "unknown type" }}<template v-if="entry.response_bytes !== undefined"> · {{ entry.response_bytes }} bytes</template></dd></div>
              <div v-if="entry.origin"><dt>Origin</dt><dd>{{ entry.origin }}</dd></div>
              <div v-if="entry.user_agent"><dt>User agent</dt><dd>{{ entry.user_agent }}</dd></div>
            </dl>
          </details>
          <p v-if="!filteredEntries.length" class="muted no-matches">No requests match this filter.</p>
        </div>

        <nav v-if="pageCount > 1" class="log-pagination" aria-label="API access log pages">
          <button class="ghost small" :disabled="currentPage <= 1" @click="currentPage--">
            <ChevronLeft :size="14" /> Previous
          </button>
          <span class="meta" aria-live="polite">Page {{ currentPage }} of {{ pageCount }}</span>
          <button class="ghost small" :disabled="currentPage >= pageCount" @click="currentPage++">
            Next <ChevronRight :size="14" />
          </button>
        </nav>
      </template>
    </div>
  </section>
</template>

<style scoped>
.api-access-entry-list {
  display: grid;
  gap: 6px;
}
.api-access-entry {
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  padding: 8px 10px;
}
.api-access-entry summary {
  display: flex;
  align-items: center;
  gap: 9px;
  cursor: pointer;
  list-style: none;
  min-width: 0;
  min-height: var(--row-height);
}
.api-access-entry summary::-webkit-details-marker {
  display: none;
}
.api-method,
.api-status {
  flex: none;
  font-size: 12px;
  line-height: 18px;
  font-weight: 700;
}
.api-method {
  min-width: 42px;
  color: var(--brand);
}
.api-method.post,
.api-method.delete,
.api-method.put {
  color: var(--warn);
}
.api-path {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.api-status.ok {
  color: var(--ok);
}
.api-status.warn {
  color: var(--warn);
}
.api-status.bad {
  color: var(--danger);
}
.api-time {
  text-align: right;
}
.api-access-chevron {
  flex: none;
  color: var(--muted);
  transition: transform var(--motion-state-duration) var(--ease);
}
.api-access-entry[open] .api-access-chevron {
  transform: rotate(90deg);
}
.api-access-details {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 12px;
  margin: 8px 0 2px 51px;
  font-size: 12px;
}
.api-access-details div {
  min-width: 0;
}
.api-access-details dt {
  color: var(--muted);
}
.api-access-details dd {
  margin: 2px 0 0;
  overflow-wrap: anywhere;
}
@media (max-width: 560px) {
  .api-time {
    display: none;
  }
  .api-access-details {
    grid-template-columns: 1fr;
    margin-left: 0;
  }
}
</style>
