<script setup lang="ts">
import { onMounted, ref } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { Pencil, RefreshCw, Trash2, X } from "lucide-vue-next";
import { api, errorText, type RemoteView } from "@/shared/api";

const emit = defineEmits<{
  (e: "notify", message: string, error?: boolean): void;
  (e: "printers-changed"): void;
}>();

const remotes = ref<RemoteView[]>([]);
const editing = ref<RemoteView | null>(null);
const name = ref("");
const url = ref("");
const token = ref("");
const preset = ref("");
const saving = ref(false);
const loading = ref(true);
const testing = ref<string | null>(null);

async function load() {
  loading.value = true;
  try {
    remotes.value = await api.remotes();
  } catch (e) {
    emit("notify", errorText(e), true);
  } finally {
    loading.value = false;
  }
}

function reset() {
  editing.value = null;
  name.value = "";
  url.value = "";
  token.value = "";
  preset.value = "";
}

function edit(remote: RemoteView) {
  editing.value = remote;
  name.value = remote.name;
  url.value = remote.url;
  token.value = "";
  preset.value = remote.preset ?? "";
}

async function submit() {
  saving.value = true;
  try {
    remotes.value = editing.value
      ? await api.remoteUpdate(editing.value.id, name.value, url.value, token.value.trim() || undefined, preset.value)
      : await api.remoteAdd(name.value, url.value, token.value, preset.value);
    emit("printers-changed");
    emit("notify", editing.value ? `Remote computer "${name.value}" updated` : `Remote computer "${name.value}" added`);
    reset();
  } catch (e) {
    emit("notify", errorText(e), true);
  } finally {
    saving.value = false;
  }
}

async function test(remote: RemoteView) {
  testing.value = remote.id;
  try {
    const next = await api.remoteTest(remote.id);
    remotes.value = remotes.value.map((item) => (item.id === next.id ? next : item));
    emit("printers-changed");
    if (next.online) emit("notify", `${next.name}: online · ${next.printers} printer${next.printers === 1 ? "" : "s"}`);
    else emit("notify", next.last_error ?? `${next.name}: remote could not be reached`, true);
  } catch (e) {
    emit("notify", errorText(e), true);
  } finally {
    testing.value = null;
  }
}

async function remove(remote: RemoteView) {
  if (
    !(await confirm(`Remove remote computer "${remote.name}"? Its printers will disappear from this computer.`, {
      title: "ix.print",
      kind: "warning",
    }))
  )
    return;
  try {
    remotes.value = await api.remoteRemove(remote.id);
    emit("printers-changed");
    emit("notify", `Remote computer "${remote.name}" removed`);
    if (editing.value?.id === remote.id) reset();
  } catch (e) {
    emit("notify", errorText(e), true);
  }
}

onMounted(load);
</script>

<template>
  <section class="stack">
    <p class="hint">Create an API token on that computer first (Settings → API access &amp; security), then paste it here.</p>
    <p v-if="loading" class="muted">Loading remote computers…</p>
    <div v-for="remote in remotes" :key="remote.id" class="card stack tight">
      <div class="row">
        <strong class="grow truncate">{{ remote.name }}</strong>
        <button class="icon" title="Test connection" :aria-label="`Test connection to ${remote.name}`" :disabled="testing === remote.id" @click="test(remote)">
          <RefreshCw :size="16" :class="{ spin: testing === remote.id }" />
        </button>
        <button class="icon" title="Edit remote computer" :aria-label="`Edit ${remote.name}`" @click="edit(remote)"><Pencil :size="16" /></button>
        <button class="icon danger" title="Remove remote computer" :aria-label="`Remove ${remote.name}`" @click="remove(remote)">
          <Trash2 :size="16" />
        </button>
      </div>
      <div class="meta truncate">
        {{ remote.url }} · token {{ remote.token_hint
        }}<template v-if="remote.preset"> · preset {{ remote.preset }}</template>
      </div>
      <div v-if="remote.online === true" class="status normal">
        Online · {{ remote.printers }} printer{{ remote.printers === 1 ? "" : "s" }}
      </div>
      <div v-else-if="remote.online === false" class="error small-text">{{ remote.last_error }}</div>
      <div v-else class="meta">Not checked yet</div>
    </div>

    <form class="card stack" @submit.prevent="submit">
      <div class="row">
        <strong class="grow">{{ editing ? "Edit remote computer" : "Add remote computer" }}</strong>
        <button v-if="editing" type="button" class="icon" title="Cancel" aria-label="Cancel editing remote computer" @click="reset"><X :size="16" /></button>
      </div>
      <label class="field"
        ><span>Name</span><input v-model="name" required maxlength="32" placeholder="e.g. PC2 or Warehouse"
      /></label>
      <label class="field"
        ><span>Address</span><input v-model="url" required placeholder="http://192.168.1.7:18032"
      /></label>
      <label class="field"
        ><span>Token</span
        ><input v-model="token" type="password" autocomplete="off" :required="!editing" placeholder="fxp_…"
      /></label>
      <p v-if="editing" class="hint small-text">Leave empty to keep the current token.</p>
      <label class="field"
        ><span>Preset on that computer <span class="meta">(optional)</span></span
        ><input v-model="preset" placeholder="e.g. warehouse-label"
      /></label>
      <div class="row end">
        <button class="primary small" type="submit" :disabled="saving">
          {{ saving ? "Connecting…" : editing ? "Save changes" : "Connect" }}
        </button>
      </div>
    </form>
  </section>
</template>

<style scoped>
.status.normal {
  color: var(--ok);
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
