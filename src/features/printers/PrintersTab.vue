<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { Copy, Info, Pencil, Plus, Printer, RefreshCw, Star } from "lucide-vue-next";
import EmptyState from "@/shared/ui/EmptyState.vue";
import PrinterDeviceForm from "./PrinterDeviceForm.vue";
import { api, errorText, type Config, type PrinterQueue, type PrinterView } from "@/shared/api";
import { describeJob } from "@/shared/lib/job";

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
const OFFLINE_MESSAGE = "Printer is offline or not responding";

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
  <Transition name="detail" mode="out-in">
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
      <span class="grow meta"
        >{{ printers.length }} printer{{ printers.length === 1 ? "" : "s" }} · updated automatically</span
      >
      <button v-if="printers.length" class="ghost small" :disabled="refreshing" title="Find new printers now" @click="emit('refresh')">
        <RefreshCw :size="14" :class="{ spin: refreshing }" /> Refresh
      </button>
      <button v-if="printers.length" class="ghost small" title="Add a printer to the system" @click="device = {}">
        <Plus :size="14" /> Add
      </button>
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

    <div
      v-for="(printer, index) in printers"
      :key="printer.name"
      class="row card printer-row"
      :class="{ active: selected === printer.name }"
    >
      <div class="printer-select grow min0">
        <input
          :id="`printer-select-${index}`"
          type="radio"
          name="printer"
          :checked="selected === printer.name"
          @change="emit('select', printer.name)"
        />
        <span class="printer-info min0 grow">
          <label class="printer-name truncate strong" :for="`printer-select-${index}`">{{ printer.name }}</label>
          <span class="meta">
            <span class="printer-status-group">
              <label class="printer-status-label" :for="`printer-select-${index}`">
                <span :class="['status', printer.status.toLowerCase()]">{{ printer.status }}</span>
              </label>
              <button
                v-if="printer.message === OFFLINE_MESSAGE"
                class="printer-message-info"
                type="button"
                aria-label="Printer status information"
                :aria-describedby="`printer-status-tooltip-${index}`"
              >
                <Info :size="14" aria-hidden="true" />
                <span :id="`printer-status-tooltip-${index}`" class="printer-message-tooltip" role="tooltip">
                  {{ printer.message }}
                </span>
              </button>
            </span>
            <label v-if="printer.os_default || printer.remote" class="printer-meta-label" :for="`printer-select-${index}`">
              <span v-if="printer.os_default" class="badge subtle">system default</span>
              <span v-if="printer.remote" class="badge subtle">via {{ printer.remote }}</span>
            </label>
          </span>
          <span
            v-if="printer.message && printer.message !== OFFLINE_MESSAGE"
            :class="printer.status === 'Normal' || printer.status === 'Printing' ? 'hint' : 'error small-text'"
          >
            {{ printer.message }}
          </span>
        </span>
      </div>
      <button
        class="icon"
        :title="config?.default_printer === printer.name ? 'Remove ix.print default printer' : 'Set as ix.print default printer'"
        :aria-label="config?.default_printer === printer.name ? 'Remove ix.print default printer' : 'Set as ix.print default printer'"
        :aria-pressed="config?.default_printer === printer.name"
        @click="emit('toggle-default', printer.name)"
      >
        <Star :size="16" :fill="config?.default_printer === printer.name ? 'currentColor' : 'none'" />
      </button>
      <button class="icon" title="Copy printer name" aria-label="Copy printer name" @click="emit('copy', printer.name)">
        <Copy :size="16" />
      </button>
      <button
        v-if="!printer.remote"
        class="icon"
        title="Edit / remove printer"
        aria-label="Edit or remove printer"
        @click="device = { name: printer.name }"
      >
        <Pencil :size="16" />
      </button>
    </div>

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
  </Transition>
</template>

<style scoped>
.printer-select {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: var(--row-height);
  cursor: pointer;
}
.printer-select input {
  flex: none;
}
.printer-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}
.printer-name,
.printer-status-label,
.printer-meta-label {
  cursor: pointer;
}
.printer-status-label,
.printer-meta-label {
  display: flex;
  align-items: center;
  gap: 6px;
}
.printer-row .meta {
  align-items: center;
}
.printer-status-group {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  flex: none;
}
.printer-message-info {
  position: relative;
  appearance: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font: inherit;
  line-height: inherit;
  text-align: left;
  flex: none;
  padding: 0;
  color: var(--danger);
  background: transparent;
  border: 0;
  cursor: help;
}
.printer-message-tooltip {
  position: absolute;
  top: 100%;
  left: 0;
  z-index: 20;
  display: block;
  width: max-content;
  max-width: min(260px, calc(100vw - 32px));
  padding: 6px 8px;
  color: var(--text);
  background: var(--surface);
  border: 1px solid var(--border-control);
  border-radius: var(--r-sm);
  font-size: 12px;
  line-height: 18px;
  font-weight: 400;
  text-align: left;
  white-space: normal;
  box-shadow: 0 2px 8px rgb(0 0 0 / 18%);
  opacity: 0;
  visibility: hidden;
  transform: translateY(var(--motion-offset));
  transition:
    opacity var(--motion-popover-duration) var(--ease),
    transform var(--motion-popover-duration) var(--ease),
    visibility 0s linear var(--motion-popover-duration);
}
.printer-message-info:hover .printer-message-tooltip,
.printer-message-info:focus .printer-message-tooltip {
  opacity: 1;
  visibility: visible;
  transform: none;
  transition-delay: 0s;
}
.printer-row .meta .badge {
  line-height: 16px;
  padding: 0 4px;
}
.printer-row .error.small-text {
  font-size: 13px;
  line-height: 19px;
}
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
@media (pointer: coarse) {
  .printer-message-info {
    min-width: 44px;
    min-height: 44px;
    margin: -10px;
  }
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
