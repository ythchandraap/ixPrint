<script setup lang="ts">
import { computed, ref, watch } from "vue";
const props = defineProps<{ localIp: string; serverPort: number; exampleBody: string; list: boolean }>();
const copied = ref(false);
const error = ref("");
const request = computed(() => `POST http://${props.localIp}:${props.serverPort}/print\nContent-Type: application/json\n\n${props.exampleBody}`);
watch(request, () => { copied.value = false; error.value = ""; });
async function copyRequest() {
  error.value = "";
  try {
    await navigator.clipboard.writeText(request.value);
    copied.value = true;
    window.setTimeout(() => { copied.value = false; }, 1800);
  } catch {
    error.value = "Could not copy the request. Select and copy the code instead.";
  }
}
</script>

<template>
  <section class="api-workspace" aria-label="API request example">
    <header class="api-heading">
      <div><h2>API</h2><p class="hint">Use the existing print endpoint to send this design with data.</p></div>
    </header>
    <div class="api-endpoint-row">
      <span class="api-method">POST</span>
      <code>http://{{ localIp }}:{{ serverPort }}/print</code>
      <span class="meta">Content-Type: application/json</span>
    </div>
    <div class="api-code-heading">
      <h3>Request example</h3>
      <button type="button" class="ghost small" @click="copyRequest">{{ copied ? "Copied" : "Copy request" }}</button>
    </div>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <pre class="api-code"><code>{{ request }}</code></pre>
    <details class="request-details">
      <summary>Request details</summary>
      <p><code>data</code> is an array of records: one record produces one {{ list ? "row" : "label" }}.</p>
      <p><code>header</code> can supply values shared by every page. Values for settings such as copies are sent in <code>settings</code>. Authentication tokens belong in the request header when the bridge requires them.</p>
    </details>
  </section>
</template>
