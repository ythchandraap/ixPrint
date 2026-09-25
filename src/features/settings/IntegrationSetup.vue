<script setup lang="ts">
import { computed, ref } from "vue";
import { api, errorText, type Config, type PrinterView } from "@/shared/api";

const props = defineProps<{ config: Config; printers: PrinterView[]; localIp: string; bridgeReady: boolean }>();
const emit = defineEmits<{
  (e: "config", config: Config): void;
  (e: "notify", message: string, error?: boolean): void;
  (e: "copy", value: string): void;
  (e: "open-api-access"): void;
}>();
const mode = ref<"browser" | "server">("browser");
const origin = ref("");
const presetName = ref("");
const checking = ref(false);
const printerCheck = ref("");
const names = computed(() => Object.keys(props.config.presets));
const selectedName = computed(() => presetName.value || props.config.default_preset || names.value[0] || "");
const preset = computed(() => props.config.presets[selectedName.value]);
const printerName = computed(() => preset.value?.printer || props.config.default_printer || props.printers.find((item) => item.os_default)?.name || "");
const printer = computed(() => props.printers.find((item) => item.name === printerName.value));
const originAllowed = computed(() => props.config.allowed_origins.includes("*") || !!origin.value.trim() && props.config.allowed_origins.includes(origin.value.trim()));
const address = computed(() => `http://${props.localIp}:${props.config.server_port}`);
const body = computed(() => ({ preset: selectedName.value, data: [preset.value?.design?.sample ?? {}] }));
const example = computed(() => {
  if (!preset.value?.design) {
    return `curl -H "Authorization: Bearer YOUR_TOKEN" -H "Idempotency-Key: UNIQUE_JOB_ID" -F file=@document.pdf "${address.value}/print?preset=${encodeURIComponent(selectedName.value)}"`;
  }
  return `const payload = ${JSON.stringify(body.value, null, 2)};
const headers = { Authorization: "Bearer YOUR_TOKEN", "Content-Type": "application/json" };
const check = await fetch(${JSON.stringify(`${address.value}/preflight`)}, {
  method: "POST", headers, body: JSON.stringify(payload)
});
const preflight = await check.json();
if (!check.ok) throw new Error(preflight.message);
if (!preflight.valid) throw new Error(preflight.errors.join("; "));
const printKey = Array.from(crypto.getRandomValues(new Uint32Array(4)), n => n.toString(16)).join("-");
// Keep printKey when retrying this logical job after a network failure.
const response = await fetch(${JSON.stringify(`${address.value}/print`)}, {
  method: "POST",
  headers: { ...headers, "Idempotency-Key": printKey },
  body: JSON.stringify(payload)
});
const result = await response.json();
if (!response.ok) throw new Error(result.message);
console.log(result.job_id, result.status);`;
});

async function allowOrigin() {
  const value = origin.value.trim();
  if (!value || originAllowed.value) return;
  try {
    emit("config", await api.setAllowedOrigins([...props.config.allowed_origins, value]));
    emit("notify", `${value} allowed`);
  } catch (error) {
    emit("notify", errorText(error), true);
  }
}
async function checkPrinter() {
  if (!printerName.value) return;
  checking.value = true;
  printerCheck.value = "";
  try {
    const capabilities = await api.capabilities(printerName.value);
    printerCheck.value = `Connected · ${capabilities.papers.length} paper choices available`;
  } catch (error) {
    printerCheck.value = errorText(error);
  } finally {
    checking.value = false;
  }
}
</script>

<template>
  <div class="card stack tight integration-setup">
    <p class="hint">Follow one published preset from design to its first request. Run browser requests from the actual web app to verify its origin and network path.</p>
    <label class="field">
      <span>Application type</span>
      <select v-model="mode"><option value="browser">Web browser</option><option value="server">Server application</option></select>
    </label>
    <label class="field">
      <span>Preset to integrate</span>
      <select v-model="presetName"><option v-for="name in names" :key="name" :value="name">{{ name }}</option></select>
    </label>
    <ul class="integration-checks">
      <li :class="bridgeReady ? 'ok-text' : 'warn-text'">{{ bridgeReady ? "✓" : "!" }} Bridge {{ bridgeReady ? "running" : "not ready" }} at {{ address }}</li>
      <li :class="config.tokens.length ? 'ok-text' : 'warn-text'">{{ config.tokens.length ? "✓" : "!" }} API token {{ config.tokens.length ? "created" : "needed" }} <a v-if="!config.tokens.length" href="#api-access" @click.prevent="emit('open-api-access')">Create one below</a></li>
      <li :class="preset ? 'ok-text' : 'warn-text'">{{ preset ? "✓" : "!" }} Preset {{ preset ? selectedName : "needed" }} <span v-if="preset?.design">· {{ preset.design.elements.length }} design elements</span></li>
      <li :class="printer ? 'ok-text' : 'warn-text'">{{ printer ? "✓" : "!" }} Printer {{ printerName || "needed" }} {{ printer?.status && printer.status !== "Ready" ? `· ${printer.status}` : "" }}</li>
    </ul>
    <div v-if="mode === 'browser'" class="stack tight">
      <p class="hint">A browser app needs access to the bridge and its API token. For production, a server-side integration usually keeps the token easier to protect.</p>
      <label class="field"><span>Web app origin</span><input v-model="origin" placeholder="https://app.example.com" /></label>
      <div class="row wrap">
        <span :class="originAllowed ? 'ok-text' : 'warn-text'">{{ originAllowed ? "✓ Origin allowed" : "! Enter and allow the exact web app origin" }}</span>
        <button type="button" class="ghost small" :disabled="!origin.trim() || originAllowed" @click="allowOrigin">Allow origin</button>
      </div>
    </div>
    <div class="row wrap">
      <button type="button" class="ghost small" :disabled="!printerName || checking" @click="checkPrinter">{{ checking ? "Checking…" : "Check printer" }}</button>
      <span v-if="printerCheck" class="small-text">{{ printerCheck }}</span>
    </div>
    <div v-if="preset" class="stack tight">
      <div class="row"><strong class="grow">Request example</strong><button type="button" class="ghost small" @click="emit('copy', example)">Copy</button></div>
      <p class="hint">Replace YOUR_TOKEN with the token shown when created. The full token is never stored in this example.</p>
      <pre class="settings">{{ example }}</pre>
      <p v-if="preset.design" class="hint">Fields: {{ Object.keys(preset.design.sample).join(", ") || "none" }} · Required: {{ preset.design.required_fields?.join(", ") || "none" }}. POST the same JSON to <code>/preflight</code> to validate without printing.</p>
    </div>
  </div>
</template>
