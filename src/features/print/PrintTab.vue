<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, FileText, Printer, SlidersHorizontal, Upload } from "lucide-vue-next";
import EmptyState from "@/shared/ui/EmptyState.vue";
import {
  api,
  errorText,
  type Config,
  type JobResult,
  type JobStatus,
  type PrinterView,
  type PrintSettings,
} from "@/shared/api";
import { describeJob, FINAL_STATES } from "@/shared/lib/job";

const props = defineProps<{
  config: Config;
  printers: PrinterView[];
  printersLoaded: boolean;
  selectedPrinter: string;
  testPages: number;
  requestedPreset: string;
  returnLabel: string;
}>();

const emit = defineEmits<{
  (e: "back"): void;
  (e: "presets"): void;
  (e: "printers"): void;
  (e: "select-printer", name: string): void;
  (e: "test-pages", pages: number): void;
  (e: "incoming", jobId: string): void;
}>();

type Source = "preset" | "pdf";
const source = ref<Source>("preset");
const names = computed(() => Object.keys(props.config.presets));

function loadRememberedPreset() {
  try {
    const name = localStorage.getItem("ixprint.testSource") ?? "";
    return props.config.presets[name] ? name : "";
  } catch {
    return "";
  }
}

const rememberedPreset = loadRememberedPreset();
const initialPreset =
  props.requestedPreset && props.config.presets[props.requestedPreset]
    ? props.requestedPreset
    : rememberedPreset ||
      (props.config.default_preset && props.config.presets[props.config.default_preset]
        ? props.config.default_preset
        : (names.value[0] ?? ""));
const preset = ref(initialPreset);
watch(
  () => props.requestedPreset,
  (name) => {
    if (name) preset.value = name;
  },
);
const selectedPreset = computed(() => (preset.value ? props.config.presets[preset.value] : undefined));
const presetRemoved = computed(() => !!preset.value && !selectedPreset.value);
const printer = computed({
  get: () => props.selectedPrinter,
  set: (name: string) => emit("select-printer", name),
});
const fileCopies = ref(1);
const pdfPath = ref("");
const fileName = computed(() => pdfPath.value.split(/[\\/]/).pop() ?? "");
const busy = ref(false);
const submitError = ref("");

watch(preset, (name) => {
  const current = name ? props.config.presets[name] : undefined;
  if (current) {
    try {
      localStorage.setItem("ixprint.testSource", name);
    } catch {
      // Storage unavailable; keep the selection for this session.
    }
  }
  submitError.value = "";
});

const effectivePrinterName = computed(
  () =>
    printer.value ||
    selectedPreset.value?.printer ||
    props.config.default_printer ||
    props.printers.find((p) => p.is_default)?.name ||
    "",
);
const effectivePrinter = computed(() => effectivePrinterName.value || "Belum ada printer yang terdeteksi");
const effectivePrinterInfo = computed(() => props.printers.find((p) => p.name === effectivePrinterName.value));
const printerErrors = computed(() => {
  if (!props.printersLoaded) return [];
  if (!effectivePrinterName.value)
    return ["Tidak ada printer yang tersedia. Hubungkan atau tambahkan printer di tab Printers."];
  if (!effectivePrinterInfo.value)
    return [`Printer “${effectivePrinterName.value}” tidak tersedia. Pilih printer lain atau periksa tab Printers.`];
  return [];
});

function settingsSummary(settings: PrintSettings, forPresetTest: boolean, hasDesign: boolean) {
  const values: string[] = [];
  if (forPresetTest && hasDesign && selectedPreset.value?.design) {
    const design = selectedPreset.value.design;
    values.push(
      `desain ${design.width_mm} × ${design.height_mm} mm`,
      "skala asli",
      "potret",
      "margin 0",
      "rata tengah",
    );
  } else {
    if (settings.paper && settings.paper !== "default") values.push(String(settings.paper));
    if (settings.orientation) values.push(String(settings.orientation));
    if (settings.scale) values.push(`skala ${settings.scale}`);
    if (settings.align) values.push(`rata ${settings.align}`);
    if (settings.margins) values.push(`margin ${settings.margins} mm`);
    if (!forPresetTest && settings.pages) values.push(`pilih halaman ${settings.pages}`);
  }
  if (!forPresetTest && settings.collate) values.push("urutkan salinan");
  if (settings.duplex && settings.duplex !== "off") values.push(`duplex ${settings.duplex}`);
  if (settings.color) values.push(settings.color === "grayscale" ? "grayscale" : "warna");
  if (settings.tray) values.push(`tray ${settings.tray}`);
  if (settings.dpi) values.push(`${settings.dpi} dpi`);
  if (settings.offset_x || settings.offset_y)
    values.push(`offset ${settings.offset_x ?? 0}, ${settings.offset_y ?? 0} mm`);
  return values.join(" · ") || "Mengikuti pengaturan default printer";
}

const currentSettings = computed(() => {
  if (!selectedPreset.value) return "Mengikuti pengaturan default printer";
  return settingsSummary(selectedPreset.value, source.value === "preset", !!selectedPreset.value.design);
});

async function chooseFile() {
  let result: string | string[] | null;
  try {
    result = await open({ multiple: false, filters: [{ name: "Dokumen", extensions: ["pdf", "png", "jpg", "jpeg"] }] });
  } catch (error) {
    submitError.value = `Tidak dapat membuka pemilih file: ${errorText(error)}`;
    return;
  }
  if (typeof result !== "string") return;
  if (!/\.(pdf|png|jpe?g)$/i.test(result)) {
    pdfPath.value = "";
    submitError.value = "Pilih satu file PDF, PNG, JPG, atau JPEG.";
    return;
  }
  pdfPath.value = result;
  submitError.value = "";
}

const validationErrors = computed(() => {
  const errors: string[] = [];
  if (source.value === "preset") {
    if (!names.value.length) errors.push("Belum ada preset. Buat atau impor preset dari tab Presets.");
    else if (!preset.value) errors.push("Pilih preset untuk mencetak sample atau halaman kalibrasi.");
    else if (presetRemoved.value) errors.push(`Preset “${preset.value}” sudah tidak tersedia. Pilih preset lain.`);
    if (!Number.isInteger(props.testPages) || props.testPages < 1 || props.testPages > 50)
      errors.push("Jumlah halaman sample/kalibrasi harus 1–50.");
  } else {
    if (!pdfPath.value) errors.push("Pilih file terlebih dahulu.");
    else if (!/\.(pdf|png|jpe?g)$/i.test(pdfPath.value)) errors.push("Pilih file PDF, PNG, JPG, atau JPEG.");
    if (!Number.isInteger(fileCopies.value) || fileCopies.value < 1 || fileCopies.value > 999)
      errors.push("Jumlah salinan PDF harus 1–999.");
  }
  return source.value === "preset" ? [...errors, ...printerErrors.value] : errors;
});
const showNoPresetErrors = computed(() => source.value === "preset" && !names.value.length);

interface PrintRun {
  result: JobResult;
  status: JobStatus | null;
  statusError: string;
  source: Source;
}
const latest = ref<PrintRun | null>(null);
let statusTimer: number | undefined;
let statusStartedAt = 0;

async function pollStatus(jobId: string) {
  try {
    const status = await api.jobStatus(jobId);
    if (latest.value?.result.job_id !== jobId) return;
    latest.value.status = status;
    latest.value.statusError = "";
    if (FINAL_STATES.includes(status.state) || Date.now() - statusStartedAt > 600_000) clearInterval(statusTimer);
  } catch (error) {
    if (latest.value?.result.job_id !== jobId) return;
    latest.value.statusError = `Status belum dapat dibaca: ${errorText(error)}. Periksa tab History.`;
    clearInterval(statusTimer);
  }
}

function follow(jobId: string) {
  clearInterval(statusTimer);
  statusStartedAt = Date.now();
  void pollStatus(jobId);
  statusTimer = window.setInterval(() => void pollStatus(jobId), 2000);
}

async function submit() {
  if (busy.value || validationErrors.value.length) return;
  clearInterval(statusTimer);
  const submittedSource = source.value;
  busy.value = true;
  submitError.value = "";
  latest.value = null;
  try {
    let result: JobResult;
    if (submittedSource === "preset") {
      result = await api.testPrint(printer.value || null, preset.value, props.testPages);
    } else {
      const incoming = await api.stageIncomingFile(pdfPath.value, "file_picker", fileCopies.value);
      emit("incoming", incoming.id);
      return;
    }
    latest.value = { result, status: null, statusError: "", source: submittedSource };
    follow(result.job_id);
  } catch (error) {
    submitError.value = errorText(error);
  } finally {
    busy.value = false;
  }
}

onBeforeUnmount(() => {
  clearInterval(statusTimer);
});
</script>

<template>
  <section class="print-form">
    <div class="print-heading">
      <div>
        <h2 class="section-title">Print</h2>
        <p class="hint">Cetak sample desain atau kirim dokumen ke pemeriksaan tujuan.</p>
      </div>
      <button class="ghost small" type="button" @click="emit('back')">
        <ArrowLeft :size="14" /> Kembali ke {{ returnLabel }}
      </button>
    </div>

    <fieldset class="source-picker">
      <legend>Sumber</legend>
      <label
        ><input v-model="source" name="print-source" type="radio" value="preset" :disabled="busy" />
        <Printer :size="15" /> Preset</label
      >
      <label
        ><input v-model="source" name="print-source" type="radio" value="pdf" :disabled="busy" />
        <FileText :size="15" /> PDF / gambar</label
      >
    </fieldset>

    <div class="grid2">
      <label v-if="source === 'preset'" class="field" for="print-printer">
        <span>Printer</span>
        <select id="print-printer" v-model="printer" :disabled="busy">
          <option value="">Otomatis · {{ effectivePrinter }}</option>
          <option v-if="printer && !props.printers.some((item) => item.name === printer)" :value="printer" disabled>
            {{ printer }} · tidak tersedia
          </option>
          <option v-for="item in printers" :key="item.name" :value="item.name">
            {{ item.name }}{{ item.status && item.status !== "Ready" ? ` · ${item.status}` : "" }}
          </option>
        </select>
      </label>
      <label v-if="source === 'preset'" class="field" for="print-preset">
        <span>Preset</span>
        <select id="print-preset" v-model="preset" :disabled="busy">
          <option value="">Pilih preset</option>
          <option v-if="preset && !selectedPreset" :value="preset" disabled>{{ preset }} · tidak tersedia</option>
          <option v-for="name in names" :key="name" :value="name">{{ name }}</option>
        </select>
      </label>
    </div>

    <label v-if="source === 'preset'" class="field count-field" for="print-test-pages">
      <span>Jumlah halaman sample / kalibrasi</span>
      <input
        id="print-test-pages"
        type="number"
        min="1"
        max="50"
        step="1"
        :value="testPages"
        :disabled="busy"
        @change="emit('test-pages', Number(($event.target as HTMLInputElement).value))"
      />
    </label>
    <label v-else class="field count-field" for="print-copies">
      <span>Jumlah salinan PDF</span>
      <input id="print-copies" v-model.number="fileCopies" type="number" min="1" max="999" step="1" :disabled="busy" />
    </label>

    <EmptyState
      v-if="source === 'preset' && !names.length"
      :icon="SlidersHorizontal"
      title="Belum ada preset"
      text="Buat atau impor preset untuk mencetak sample desain atau halaman kalibrasi."
    >
      <button class="primary small" type="button" @click="emit('presets')">Buka Presets</button>
    </EmptyState>

    <div v-else-if="source === 'preset'" class="source-content">
      <p v-if="selectedPreset?.design" class="hint source-note">
        Preset berdesain mencetak <code>design.sample</code> yang tersimpan. Untuk preset tanpa desain, yang dicetak
        adalah halaman kalibrasi memakai pengaturan preset.
      </p>
      <p v-else class="hint source-note">
        {{
          selectedPreset
            ? "Preset ini tidak memiliki desain; yang dicetak adalah halaman kalibrasi memakai pengaturan preset."
            : "Pilih preset yang tersedia."
        }}
      </p>
    </div>

    <div v-else class="source-content">
      <div class="pdf-picker">
        <div class="truncate">
          <strong>{{ fileName || "Belum ada file dipilih" }}</strong>
          <p class="hint">Pilih satu PDF, PNG, atau JPG. Dokumen diperiksa dan dipratinjau sebelum dikirim.</p>
        </div>
        <button class="ghost small" type="button" :disabled="busy" @click="chooseFile">
          <Upload :size="14" /> Pilih file
        </button>
      </div>
      <p class="hint source-note">DOCX/XLSX: cetak dari Word/Excel dengan memilih printer ix.print.</p>
    </div>

    <section class="summary" aria-label="Ringkasan print">
      <strong>Ringkasan</strong>
      <div>
        <span>Sumber</span><b>{{ source === "preset" ? "Preset" : fileName || "File belum dipilih" }}</b>
      </div>
      <div v-if="source === 'preset'">
        <span>Printer</span
        ><b
          >{{ effectivePrinter
          }}<template v-if="effectivePrinterInfo?.status && effectivePrinterInfo.status !== 'Ready'">
            · {{ effectivePrinterInfo.status }}</template
          ></b
        >
      </div>
      <div v-if="source === 'preset'">
        <span>Preset</span><b>{{ source === "preset" ? preset || "Belum dipilih" : preset || "Tanpa preset" }}</b>
      </div>
      <div v-if="source === 'preset'">
        <span>Halaman sample / kalibrasi</span><b>{{ testPages }}</b>
      </div>
      <div v-else>
        <span>Salinan</span><b>{{ fileCopies }}</b>
      </div>
      <div v-if="source === 'preset'">
        <span>Pengaturan efektif</span><b>{{ currentSettings }}</b>
      </div>
    </section>

    <ul v-if="validationErrors.length && !showNoPresetErrors" class="validation-errors" aria-live="polite">
      <li v-for="error in validationErrors" :key="error">{{ error }}</li>
    </ul>
    <div v-if="source === 'preset' && printerErrors.length" class="inline-action">
      <button class="ghost small" type="button" @click="emit('printers')">Buka Printers</button>
    </div>
    <p v-if="presetRemoved" class="error" role="alert">Preset “{{ preset }}” sudah dihapus atau tidak lagi tersedia.</p>
    <p v-if="submitError" class="error submit-error" role="alert">{{ submitError }}</p>

    <div class="print-actions">
      <button class="primary" type="button" :disabled="busy || validationErrors.length > 0" @click="submit">
        <Printer :size="15" />
        {{ busy ? "Menyiapkan…" : source === "preset" ? "Print sample / kalibrasi" : "Tinjau tujuan" }}
      </button>
    </div>
    <p v-if="source === 'pdf'" class="hint action-note">
      Ukuran, orientasi, dan skala dokumen dipertahankan. Konfirmasi tujuan dilakukan di layar berikutnya.
    </p>

    <article v-if="latest" class="job-result" aria-live="polite">
      <div class="job-result-heading">
        <strong>{{ latest.source === "preset" ? "Sample / kalibrasi dikirim" : "PDF dikirim" }}</strong>
        <span :class="['state', latest.status?.state ?? 'queued']">{{
          latest.status ? describeJob(latest.status) : "Memeriksa status…"
        }}</span>
      </div>
      <dl>
        <div>
          <dt>job_id</dt>
          <dd>{{ latest.result.job_id }}</dd>
        </div>
        <div>
          <dt>Printer</dt>
          <dd>{{ latest.result.printer }}</dd>
        </div>
        <div>
          <dt>Jumlah lembar</dt>
          <dd>{{ latest.result.sheets }}</dd>
        </div>
        <div v-if="latest.status?.position">
          <dt>Posisi antrean</dt>
          <dd>{{ latest.status.position }}</dd>
        </div>
      </dl>
      <p v-if="latest.status?.state === 'error'" class="error">
        {{ latest.status.message || latest.status.printer_message || "Periksa printer, kertas, dan antreannya." }}
      </p>
      <p v-if="latest.statusError" class="error">{{ latest.statusError }}</p>
    </article>
  </section>
</template>

<style scoped>
.print-form {
  display: flex;
  flex-direction: column;
  gap: var(--stack-gap);
  padding: 12px 12px 16px;
}
.print-heading {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--row-gap);
}
.source-picker {
  display: flex;
  gap: 12px;
  margin: 0;
  padding: 8px 10px 10px;
  border: 1px solid var(--line);
  border-radius: var(--r-md);
}
.source-picker label {
  display: inline-flex;
  align-items: center;
  min-height: var(--row-height);
  gap: 6px;
  color: var(--text);
  font-size: 14px;
  cursor: pointer;
}
.count-field {
  max-width: 280px;
}
.count-field input {
  max-width: 140px;
}
.source-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.source-note {
  margin: 0;
}
.pdf-picker,
.print-actions,
.job-result-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.pdf-picker {
  min-height: 64px;
  padding: 8px 10px;
  border: 1px dashed var(--line);
  border-radius: var(--r-md);
}
.pdf-picker strong {
  display: block;
  font-size: 14px;
}
.pdf-picker .hint {
  margin-top: 3px;
}
.pdf-preview {
  display: flex;
  justify-content: center;
  padding: 8px;
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  background: var(--surface);
}
.pdf-preview img {
  max-width: 100%;
  max-height: 360px;
  box-shadow: 0 0 0 1px var(--line);
}
.summary {
  display: grid;
  gap: 6px;
  padding: 8px 10px;
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: var(--r-md);
}
.summary > strong {
  margin-bottom: 2px;
  font-size: 14px;
}
.summary > div {
  display: grid;
  grid-template-columns: 160px minmax(0, 1fr);
  gap: 8px;
  font-size: 12px;
  line-height: 18px;
}
.summary span {
  color: var(--muted);
}
.summary b {
  font-weight: 500;
  overflow-wrap: anywhere;
}
.validation-errors {
  margin: -4px 0 0;
  padding-left: 20px;
  color: var(--danger);
  font-size: 14px;
  line-height: 21px;
}
.inline-action {
  display: flex;
  justify-content: flex-start;
  margin-top: -4px;
}
.submit-error {
  margin: -4px 0 0;
}
.print-actions button {
  flex: 1;
  justify-content: center;
}
.action-note {
  text-align: center;
}
.job-result {
  padding: 8px 10px;
  border: 1px solid var(--line);
  border-radius: var(--r-md);
  background: var(--surface);
}
.job-result-heading strong {
  font-size: 14px;
}
.job-result dl {
  display: grid;
  gap: 5px;
  margin: 8px 0 0;
}
.job-result dl > div {
  display: grid;
  grid-template-columns: 112px minmax(0, 1fr);
  gap: 8px;
  font-size: 12px;
  line-height: 18px;
}
.job-result dt {
  color: var(--muted);
}
.job-result dd {
  margin: 0;
  font-family: var(--font-mono);
  overflow-wrap: anywhere;
}
.state {
  color: var(--muted);
  font-size: 12px;
  line-height: 18px;
}
.state.error,
.state.failed {
  color: var(--danger);
}
.state.completed {
  color: var(--ok);
}
.job-result .error {
  margin: 8px 0 0;
}
@media (max-width: 520px) {
  .print-heading {
    align-items: flex-start;
    flex-direction: column;
  }
  .summary > div {
    grid-template-columns: 132px minmax(0, 1fr);
  }
}
</style>
