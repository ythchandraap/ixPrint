<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ArrowLeft, ArrowRight, Check, Printer, X } from "lucide-vue-next";
import {
  api,
  errorText,
  type IncomingJob,
  type IncomingTargetCheck,
  type JobStatus,
  type PrinterView,
  type RemoteView,
} from "@/shared/api";

const props = defineProps<{
  initialJobId: string;
  printers: PrinterView[];
}>();

const emit = defineEmits<{ (e: "close"): void; (e: "refresh-printers"): void }>();

const jobs = ref<IncomingJob[]>([]);
const remotes = ref<RemoteView[]>([]);
const activeId = ref(props.initialJobId);
const location = ref("");
const printerName = ref("");
const page = ref(1);
const preview = ref("");
const previewError = ref("");
const check = ref<IncomingTargetCheck | null>(null);
const queueStatus = ref<JobStatus | null>(null);
const queueStatusError = ref("");
const busy = ref(false);
const message = ref("");
const error = ref("");
const reviewHeading = ref<HTMLHeadingElement | null>(null);
let refreshTimer: number | undefined;
let previewSeq = 0;
let targetSeq = 0;
let queueTimer: number | undefined;

const pendingJobs = computed(() =>
  jobs.value.filter((job) => job.state === "awaiting_destination" || job.state === "submitting"),
);
const activeJob = computed(() => jobs.value.find((job) => job.id === activeId.value) ?? null);
const locations = computed(() => {
  const names = new Set([
    ...props.printers.map((printer) => printer.remote).filter((name): name is string => !!name),
    ...remotes.value.map((remote) => remote.name),
  ]);
  return [{ id: "", label: "Komputer ini" }, ...[...names].map((name) => ({ id: name, label: name }))];
});
const printersAtLocation = computed(() =>
  props.printers.filter((printer) => (printer.remote ?? "") === location.value),
);
const chosenPrinter = computed(
  () => printersAtLocation.value.find((printer) => printer.name === printerName.value) ?? null,
);
const dimensions = computed(() => {
  if (!activeJob.value) return "";
  const unique = new Set(
    activeJob.value.pages.map((item) => {
      const width = ((item.width_pt * 25.4) / 72).toFixed(0);
      const height = ((item.height_pt * 25.4) / 72).toFixed(0);
      return `${width} × ${height} mm`;
    }),
  );
  return [...unique].join(", ");
});
const canSubmit = computed(
  () =>
    !!activeJob.value &&
    activeJob.value.state === "awaiting_destination" &&
    !!printerName.value &&
    check.value?.compatible === true &&
    !busy.value,
);

async function refresh() {
  try {
    jobs.value = await api.incomingDocuments();
    if (!activeId.value || !jobs.value.some((job) => job.id === activeId.value)) {
      activeId.value = pendingJobs.value[0]?.id ?? jobs.value[0]?.id ?? "";
    }
  } catch (cause) {
    error.value = `Antrean dokumen tidak dapat dibaca: ${errorText(cause)}`;
  }
}

async function loadRemotes() {
  try {
    remotes.value = await api.remotes();
  } catch {
    remotes.value = [];
  }
}

watch(
  () => props.initialJobId,
  (id) => {
    if (id) activeId.value = id;
  },
);
watch(activeId, () => {
  location.value = "";
  printerName.value = "";
  page.value = 1;
  message.value = "";
  error.value = "";
  check.value = null;
});
watch([activeJob, page], async ([job, pageNumber]) => {
  const seq = ++previewSeq;
  preview.value = "";
  previewError.value = "";
  check.value = null;
  if (!job || job.state !== "awaiting_destination") return;
  try {
    preview.value = await api.previewIncomingPdfPage(job.id, pageNumber);
  } catch (cause) {
    if (seq === previewSeq) previewError.value = errorText(cause);
  }
});
watch([activeJob, printerName], async ([job, selected]) => {
  const seq = ++targetSeq;
  check.value = null;
  error.value = "";
  if (!job || job.state !== "awaiting_destination" || !selected) return;
  try {
    check.value = await api.checkIncomingTarget(job.id, selected);
  } catch (cause) {
    if (seq === targetSeq) error.value = errorText(cause);
  }
});
watch(location, async () => {
  printerName.value = "";
  check.value = null;
  if (location.value) emit("refresh-printers");
});
watch(
  () => `${activeJob.value?.id ?? ""}:${activeJob.value?.state ?? ""}`,
  (key) => {
    window.clearInterval(queueTimer);
    queueStatus.value = null;
    queueStatusError.value = "";
    const [jobId, state] = key.split(":");
    if (!jobId || state !== "queued") return;
    void pollQueueStatus(jobId);
    queueTimer = window.setInterval(() => void pollQueueStatus(jobId), 2500);
  },
);

async function pollQueueStatus(jobId: string) {
  try {
    const status = await api.jobStatus(jobId);
    if (activeJob.value?.id !== jobId) return;
    queueStatus.value = status;
    queueStatusError.value = "";
    if (["completed", "gone", "failed"].includes(status.state)) window.clearInterval(queueTimer);
  } catch (cause) {
    if (activeJob.value?.id !== jobId) return;
    queueStatusError.value = errorText(cause);
    window.clearInterval(queueTimer);
  }
}

function queueStatusLabel(status: JobStatus) {
  switch (status.state) {
    case "queued":
      return status.position ? `Menunggu · posisi ${status.position}` : "Menunggu di antrean";
    case "printing":
      return "Sedang dicetak";
    case "paused":
      return "Dijeda printer";
    case "error":
      return `Gangguan printer${status.message ? ` · ${status.message}` : ""}`;
    case "completed":
      return "Spooler melaporkan selesai; hasil fisik tidak dapat dipastikan";
    case "gone":
      return "Keluar dari antrean; hasil cetak tidak diketahui";
    case "failed":
      return `Gagal${status.message ? ` · ${status.message}` : ""}`;
    default:
      return "Status belum diketahui";
  }
}

async function submit() {
  if (!canSubmit.value || !activeJob.value) return;
  busy.value = true;
  error.value = "";
  message.value = "Mengirim ke antrean printer…";
  try {
    const result = await api.submitIncomingDocument(activeJob.value.id, printerName.value);
    message.value = `Dikirim ke antrean ${result.printer}${result.remote ? ` di ${result.remote}` : ""}. ix.print tidak dapat memastikan kertas sudah tercetak.`;
    await refresh();
  } catch (cause) {
    error.value = errorText(cause);
    message.value = "";
    await refresh();
  } finally {
    busy.value = false;
  }
}

async function cancel() {
  if (!activeJob.value || activeJob.value.state !== "awaiting_destination") return;
  busy.value = true;
  try {
    await api.cancelIncomingDocument(activeJob.value.id);
    message.value = "Dokumen dibatalkan sebelum dikirim ke printer.";
    await refresh();
  } catch (cause) {
    error.value = errorText(cause);
  } finally {
    busy.value = false;
  }
}

onMounted(() => {
  void nextTick(() => reviewHeading.value?.focus());
  void refresh();
  void loadRemotes();
  refreshTimer = window.setInterval(() => void refresh(), 2500);
});
onBeforeUnmount(() => {
  window.clearInterval(refreshTimer);
  window.clearInterval(queueTimer);
});
</script>

<template>
  <section class="incoming-review">
    <header class="review-heading">
      <div>
        <h2 ref="reviewHeading" class="section-title" tabindex="-1">Pemeriksaan tujuan</h2>
        <p class="hint">Periksa dokumen dan pilih printer sebelum mengirim job.</p>
      </div>
      <button class="ghost small" type="button" @click="emit('close')"><ArrowLeft :size="14" /> Kembali</button>
    </header>

    <nav v-if="pendingJobs.length > 1" class="incoming-list" aria-label="Dokumen menunggu tujuan">
      <button
        v-for="job in pendingJobs"
        :key="job.id"
        type="button"
        :class="{ selected: activeId === job.id }"
        @click="activeId = job.id"
      >
        <span class="truncate">{{ job.document_name }}</span
        ><small>{{ job.pages.length }} hlm</small>
      </button>
    </nav>

    <template v-if="activeJob">
      <section class="incoming-summary" aria-label="Ringkasan dokumen">
        <div class="document-title">
          <strong>{{ activeJob.document_name }}</strong
          ><span :class="['state', activeJob.state]">{{
            activeJob.state === "awaiting_destination"
              ? "Menunggu tujuan"
              : activeJob.state === "submitting"
                ? "Memproses pengiriman"
                : activeJob.state === "queued"
                  ? "Dikirim ke antrean"
                  : activeJob.state === "failed"
                    ? "Gagal"
                    : activeJob.state === "cancelled"
                      ? "Dibatalkan"
                      : "Sedang dikirim"
          }}</span>
        </div>
        <dl>
          <div>
            <dt>Sumber</dt>
            <dd>{{ activeJob.source.replace(/_/g, " ") }}</dd>
          </div>
          <div>
            <dt>Halaman</dt>
            <dd>{{ activeJob.pages.length }}</dd>
          </div>
          <div>
            <dt>Ukuran halaman</dt>
            <dd>{{ dimensions }}</dd>
          </div>
          <div>
            <dt>Salinan dari sumber</dt>
            <dd>{{ activeJob.copies }}</dd>
          </div>
          <div v-if="activeJob.printer">
            <dt>Tujuan</dt>
            <dd>{{ activeJob.printer }}</dd>
          </div>
          <div>
            <dt>Job ID</dt>
            <dd class="mono">{{ activeJob.id }}</dd>
          </div>
          <div v-if="queueStatus">
            <dt>Status antrean</dt>
            <dd>{{ queueStatusLabel(queueStatus) }}</dd>
          </div>
          <div v-if="activeJob.spooler_job">
            <dt>Job antrean</dt>
            <dd class="mono">{{ activeJob.spooler_job }}</dd>
          </div>
          <div v-if="activeJob.remote_job_id">
            <dt>Job ID remote</dt>
            <dd class="mono">{{ activeJob.remote_job_id }}</dd>
          </div>
        </dl>
      </section>

      <section
        v-if="activeJob.state === 'awaiting_destination'"
        class="incoming-preview"
        aria-label="Pratinjau halaman"
      >
        <div class="preview-controls">
          <button
            class="ghost small"
            type="button"
            :disabled="page <= 1"
            aria-label="Halaman sebelumnya"
            @click="page--"
          >
            <ArrowLeft :size="14" />
          </button>
          <span>Halaman {{ page }} / {{ activeJob.pages.length }}</span>
          <button
            class="ghost small"
            type="button"
            :disabled="page >= activeJob.pages.length"
            aria-label="Halaman berikutnya"
            @click="page++"
          >
            <ArrowRight :size="14" />
          </button>
        </div>
        <p v-if="previewError" class="error" role="alert">Pratinjau gagal: {{ previewError }}</p>
        <p v-else-if="!preview" class="hint">Memuat pratinjau…</p>
        <img v-else :src="preview" :alt="`Pratinjau halaman ${page}`" />
      </section>

      <template v-if="activeJob.state === 'awaiting_destination'">
        <label class="field" for="incoming-location"
          ><span>Komputer tujuan</span>
          <select id="incoming-location" v-model="location">
            <option v-for="item in locations" :key="item.id" :value="item.id">{{ item.label }}</option>
          </select>
        </label>
        <label class="field" for="incoming-printer"
          ><span>Printer di lokasi terpilih</span>
          <select id="incoming-printer" v-model="printerName" :disabled="!printersAtLocation.length">
            <option value="">Pilih printer</option>
            <option
              v-for="printer in printersAtLocation"
              :key="printer.name"
              :value="printer.name"
              :disabled="
                printer.status.toLowerCase().includes('offline') || printer.status.toLowerCase().includes('disabled')
              "
            >
              {{ printer.name.split(" / ")[printer.name.split(" / ").length - 1]
              }}{{ printer.status && printer.status !== "Ready" ? ` · ${printer.status}` : "" }}
            </option>
          </select>
        </label>
        <p v-if="location && !printersAtLocation.length" class="error">
          {{
            remotes.find((remote) => remote.name === location)?.online === false
              ? `Komputer ${location} sedang offline.`
              : `Belum ada printer tersedia dari ${location}.`
          }}
        </p>
        <p v-if="chosenPrinter?.message" class="hint">{{ chosenPrinter.message }}</p>
        <p v-if="printerName && !check && !error" class="hint">Memeriksa dukungan ukuran kertas…</p>
        <p v-if="check?.supported_media.length" class="hint">
          Media yang didukung: {{ check.supported_media.join(", ") }}
        </p>
        <p v-if="check && !check.compatible" class="error" role="alert">{{ check.message }}</p>
      </template>
      <p v-else-if="activeJob.message" class="hint">{{ activeJob.message }}</p>
      <p v-else-if="activeJob.state === 'submitting'" class="hint">
        Status pengiriman belum pasti. Periksa History dan antrean printer; job tidak akan dikirim ulang otomatis.
      </p>
      <p v-if="queueStatusError" class="hint">Status antrean tidak dapat dibaca: {{ queueStatusError }}</p>

      <p v-if="message" class="success-message" role="status"><Check :size="16" /> {{ message }}</p>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <footer class="review-actions">
        <button
          v-if="activeJob.state === 'awaiting_destination'"
          class="ghost"
          type="button"
          :disabled="busy"
          @click="cancel"
        >
          <X :size="15" /> Batal
        </button>
        <button
          v-if="activeJob.state === 'awaiting_destination'"
          class="primary"
          type="button"
          :disabled="!canSubmit"
          @click="submit"
        >
          <Printer :size="15" /> {{ busy ? "Mengirim…" : "Kirim cetak" }}
        </button>
        <button v-else class="primary" type="button" @click="emit('close')">Selesai</button>
      </footer>
    </template>
    <p v-else class="hint">Belum ada dokumen yang menunggu pilihan tujuan.</p>
  </section>
</template>

<style scoped>
.incoming-review {
  display: flex;
  flex-direction: column;
  gap: var(--stack-gap);
  padding: 12px 12px 16px;
}
.review-heading,
.document-title,
.preview-controls,
.review-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.incoming-list {
  display: flex;
  gap: 6px;
  overflow-x: auto;
}
.incoming-list button {
  min-width: 130px;
  max-width: 220px;
  min-height: 44px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  background: var(--surface);
  color: var(--text);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  padding: 6px 8px;
}
.incoming-list button.selected {
  border-color: var(--brand);
}
.incoming-list small {
  color: var(--muted);
  white-space: nowrap;
}
.incoming-summary,
.incoming-preview {
  display: grid;
  gap: 8px;
  padding: 9px 10px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
}
.document-title strong {
  overflow-wrap: anywhere;
}
.incoming-summary dl {
  display: grid;
  gap: 5px;
  margin: 0;
}
.incoming-summary dl > div {
  display: grid;
  grid-template-columns: 120px minmax(0, 1fr);
  gap: 8px;
  font-size: 12px;
  line-height: 18px;
}
.incoming-summary dt {
  color: var(--muted);
}
.incoming-summary dd {
  margin: 0;
  overflow-wrap: anywhere;
}
.mono {
  font-family: var(--font-mono);
}
.preview-controls {
  justify-content: center;
  min-height: 44px;
  color: var(--muted);
  font-size: 13px;
}
.preview-controls button,
.review-heading > button {
  min-width: 44px;
  min-height: 44px;
  justify-content: center;
}
.incoming-preview img {
  max-width: 100%;
  max-height: 320px;
  margin: 0 auto;
  box-shadow: 0 0 0 1px var(--line);
}
.success-message {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin: 0;
  color: var(--ok);
  font-size: 13px;
  line-height: 19px;
}
.review-actions button {
  flex: 1;
  justify-content: center;
}
.state {
  color: var(--muted);
  font-size: 12px;
}
.state.failed {
  color: var(--danger);
}
.state.queued {
  color: var(--ok);
}
@media (max-width: 520px) {
  .review-heading {
    align-items: flex-start;
    flex-direction: column;
  }
  .incoming-summary dl > div {
    grid-template-columns: 108px minmax(0, 1fr);
  }
}
</style>
