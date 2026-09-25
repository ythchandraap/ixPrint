<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from "vue";
import { Activity, ArrowRight, BookOpen, ExternalLink, FolderOpen, Monitor, Network, Printer, Server, ShieldCheck } from "lucide-vue-next";
import { api, errorText, type Config } from "@/shared/api";
import type { Appearance } from "@/app/appearance";
import SettingInfo from "@/shared/ui/SettingInfo.vue";
import IntegrationSetup from "./IntegrationSetup.vue";
import type { PrinterView } from "@/shared/api";

type SettingsMenu = "appearance" | "integration" | "api-access" | "remote-printers" | "local-data" | "diagnostics";
type DiagnosticsLog = "error-log" | "api-access-log";

const props = defineProps<{
  config: Config | null;
  configError: string;
  localIp: string;
  os: string;
  printers: PrinterView[];
  bridgeReady: boolean;
  appearance: Appearance;
  activeMenu: SettingsMenu | null;
  diagnosticsLog: DiagnosticsLog;
}>();
const emit = defineEmits<{
  (e: "config", config: Config): void;
  (e: "notify", message: string, error?: boolean): void;
  (e: "copy", text: string): void;
  (e: "docs", external: boolean): void;
  (e: "printers-changed"): void;
  (e: "appearance-change", appearance: Appearance): void;
  (e: "menu-change", menu: SettingsMenu | null): void;
  (e: "diagnostics-log-change", log: DiagnosticsLog): void;
}>();

const menuItems = [
  { id: "appearance", title: "Appearance", description: "Choose a light, dark or system theme.", icon: Monitor },
  { id: "integration", title: "Integration & connection", description: "Setup, bridge, connection and documentation.", icon: Network },
  { id: "api-access", title: "API access & security", description: "Tokens and access controls for API requests.", icon: ShieldCheck },
  { id: "remote-printers", title: "Remote printers", description: "Printers shared by another ix.print computer.", icon: Printer },
  { id: "local-data", title: "Local data", description: "Inspect and manage this computer’s ix.print files.", icon: FolderOpen },
  { id: "diagnostics", title: "Diagnostics", description: "Review application and API activity logs.", icon: Activity },
] as const;

function onAppearanceChange(event: Event) {
  const value = (event.currentTarget as HTMLSelectElement).value;
  if (value === "system" || value === "light" || value === "dark") {
    emit("appearance-change", value);
  }
}

const port = ref("");
const savingPort = ref(false);
const savingLanAccess = ref(false);
watch(
  () => props.config?.server_port,
  (value) => { if (value !== undefined) port.value = String(value); },
  { immediate: true },
);

watch(() => props.activeMenu, async () => {
  await nextTick();
  window.scrollTo({ top: 0, behavior: "auto" });
});

async function savePort() {
  const config = props.config;
  if (!config) return;
  const value = Number(port.value.trim());
  if (!Number.isInteger(value) || value < 1024 || value > 65535) {
    emit("notify", "Server port must be a whole number between 1024 and 65535", true);
    return;
  }
  savingPort.value = true;
  try {
    emit("config", await api.setServerPort(value));
    emit(
      "notify",
      config.bridge_enabled
        ? `Server port changed to ${value}; the application bridge is restarting`
        : `Server port changed to ${value}`,
    );
  } catch (e) {
    emit("notify", errorText(e), true);
  } finally {
    savingPort.value = false;
  }
}

async function toggleLanAccess(event: Event) {
  const config = props.config;
  if (!config) return;
  const input = event.target as HTMLInputElement;
  savingLanAccess.value = true;
  try {
    const next = await api.setLanAccess(input.checked);
    emit("config", next);
    const message = input.checked ? "LAN access enabled" : "LAN access disabled";
    emit("notify", config.bridge_enabled ? `${message}; the bridge is restarting` : message);
  } catch (e) {
    input.checked = config.lan_access;
    emit("notify", errorText(e), true);
  } finally {
    savingLanAccess.value = false;
  }
}

const dataFolderPath = ref("");
onMounted(async () => {
  try {
    dataFolderPath.value = await api.dataFolderPath();
  } catch {
    // Shown as empty; the Open/Copy buttons below still work.
  }
});

async function openDataFolder() {
  try {
    await api.openDataFolder();
  } catch (e) {
    emit("notify", errorText(e), true);
  }
}
</script>

<template>
  <section class="settings-page stack">
    <div v-if="configError && activeMenu === 'appearance'" class="settings-config-error" role="alert">
      <p><strong>Settings could not be loaded.</strong> {{ configError }}</p>
      <button class="ghost small" type="button" @click="emit('menu-change', 'diagnostics')">Open Diagnostics</button>
    </div>

    <Transition name="detail" mode="out-in">
    <div v-if="!activeMenu" key="settings-menu" class="settings-content stack">
      <div v-if="configError" class="settings-config-error" role="alert">
        <p><strong>Settings could not be loaded.</strong> {{ configError }}</p>
        <button class="ghost small" type="button" @click="emit('menu-change', 'diagnostics')">Open Diagnostics</button>
      </div>
      <nav class="settings-menu" aria-label="Settings">
        <button
          v-for="item in menuItems"
          :key="item.id"
          class="settings-menu-item"
          type="button"
          @click="emit('menu-change', item.id)"
        >
          <component :is="item.icon" class="settings-menu-icon" :size="18" aria-hidden="true" />
          <span class="settings-menu-copy">
            <strong>{{ item.title }}</strong>
            <small>{{ item.description }}</small>
          </span>
          <ArrowRight class="settings-menu-arrow" :size="16" aria-hidden="true" />
        </button>
      </nav>
    </div>

    <section v-else-if="activeMenu !== 'appearance' && activeMenu !== 'diagnostics' && !config" key="config-unavailable" class="settings-config-error" role="alert">
      <p v-if="configError"><strong>Settings could not be loaded.</strong> {{ configError }}</p>
      <p v-else>Loading settings…</p>
      <button v-if="configError" class="ghost small" type="button" @click="emit('menu-change', 'diagnostics')">Open Diagnostics</button>
      <button v-else class="ghost small" type="button" @click="emit('menu-change', null)">Back to Settings</button>
    </section>

    <section v-else-if="activeMenu === 'diagnostics'" key="diagnostics-detail" class="settings-group diagnostics-settings">
      <div class="settings-log-choices" role="group" aria-label="Diagnostics log">
        <button
          type="button"
          :class="{ active: diagnosticsLog === 'error-log' }"
          :aria-pressed="diagnosticsLog === 'error-log'"
          @click="emit('diagnostics-log-change', 'error-log')"
        >Error log</button>
        <button
          type="button"
          :class="{ active: diagnosticsLog === 'api-access-log' }"
          :aria-pressed="diagnosticsLog === 'api-access-log'"
          @click="emit('diagnostics-log-change', 'api-access-log')"
        >API access log</button>
      </div>
      <Transition name="detail" mode="out-in">
        <slot name="diagnostics" :show-system-logs="props.os === 'windows'" :active-log="diagnosticsLog" />
      </Transition>
    </section>

    <section v-else-if="activeMenu === 'appearance'" key="appearance-detail" class="settings-group">
      <div class="settings-group-heading">
        <div class="settings-group-title">
          <Monitor :size="18" />
          <div>
            <h2>Appearance</h2>
            <p>Choose how ix.print looks on this computer.</p>
          </div>
        </div>
      </div>
      <label class="field" for="appearance-theme">
        <span>Theme</span>
        <select
          id="appearance-theme"
          class="appearance-select"
          :value="appearance"
          aria-describedby="appearance-theme-hint"
          @change="onAppearanceChange"
        >
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </label>
      <p id="appearance-theme-hint" class="hint">System follows your device appearance.</p>
    </section>

    <div v-else :key="activeMenu" class="settings-content stack">
    <section v-if="config && activeMenu === 'integration'" class="settings-group">
      <div class="settings-group-heading"><div class="settings-group-title"><Network :size="18" /><div><h2>Integration setup</h2><p>Connect an application to one tested preset.</p></div></div></div>
      <IntegrationSetup :config="config" :printers="printers" :local-ip="localIp" :bridge-ready="bridgeReady" @config="emit('config', $event)" @notify="(message, error) => emit('notify', message, error)" @copy="emit('copy', $event)" @open-api-access="emit('menu-change', 'api-access')" />
    </section>

    <section v-if="config && activeMenu === 'integration'" class="settings-group">
      <div class="settings-group-heading">
        <div class="settings-group-title">
          <Server :size="18" />
          <div>
            <h2>Bridge &amp; connection</h2>
            <p>Control the local HTTP service used by web apps and API tools.</p>
          </div>
        </div>
        <SettingInfo title="Bridge & connection">
          <p>The bridge is the local service between a web app and the printer system.</p>
          <ol>
            <li>A web app sends a request to this computer’s IP address and server port.</li>
            <li>ix.print checks the browser origin and API token, then resolves the preset and printer.</li>
            <li>The operating system printer service receives the job and ix.print records its status in History.</li>
          </ol>
          <p class="info-note">Turning the bridge off stops new API requests but keeps the ix.print window open.</p>
        </SettingInfo>
      </div>
      <div class="card stack tight">
        <label class="row check">
          <input type="checkbox" :checked="config.lan_access" :disabled="savingLanAccess" @change="toggleLanAccess" />
          <strong class="grow">Allow other computers on the network</strong>
        </label>
        <p class="hint">
          Turn off to accept requests from this computer only. Printer forwarding from other ix.print computers stops
          working.
        </p>
      </div>

      <fieldset>
        <legend>Server port</legend>
        <label class="field" for="server-port">
          <span>Port used by web apps and the API</span>
          <div class="row">
            <input
              id="server-port"
              v-model="port"
              class="port-input grow"
              inputmode="numeric"
              pattern="[0-9]*"
              autocomplete="off"
            />
            <button class="primary small" type="button" :disabled="savingPort" @click="savePort">
              {{ savingPort ? "Saving…" : "Save port" }}
            </button>
          </div>
        </label>
        <p class="hint">Restarts the bridge after a change.</p>
      </fieldset>
    </section>

    <section v-if="config && activeMenu === 'api-access'" id="api-access" class="settings-group">
      <div class="settings-group-heading">
        <div class="settings-group-title">
          <ShieldCheck :size="18" />
          <div>
            <h2>API access &amp; security</h2>
            <p>Protect print requests and limit which browser apps may connect.</p>
          </div>
        </div>
        <SettingInfo title="API access & security">
          <p>This group controls who can call the local API. It does not change direct OS printer permissions.</p>
          <ol>
            <li>For browser requests, ix.print checks the web app origin against Allowed websites.</li>
            <li>It checks the API token when Require a token is enabled.</li>
            <li>Only then can the request list printers, inspect queues or submit a print job.</li>
          </ol>
          <p>
            Allowed websites only limits browser (JavaScript fetch) calls — server-side apps, curl and other non-browser
            clients are not restricted by it.
          </p>
          <p class="info-note">Tokens are stored as hashes. The full token is shown only once when created.</p>
        </SettingInfo>
      </div>
      <slot name="api" :config="config" :local-ip="localIp" />
    </section>

    <section v-if="config && activeMenu === 'remote-printers'" class="settings-group">
      <div class="settings-group-heading">
        <div class="settings-group-title">
          <Network :size="18" />
          <div>
            <h2>Remote printers</h2>
            <p>Use printers connected to another ix.print computer on your network.</p>
          </div>
        </div>
        <SettingInfo title="Remote printers">
          <p>A remote computer is another ix.print instance that you explicitly add with its address and API token.</p>
          <ol>
            <li>ix.print tests the address and token before saving the remote.</li>
            <li>Its local printers appear as <code>Remote name / Printer name</code>.</li>
            <li>A selected job is forwarded directly to that computer, which prints it and reports status back.</li>
          </ol>
          <p class="info-note">
            Remote forwarding is limited to one hop. Use HTTPS or a trusted LAN when sending tokens and print data.
          </p>
        </SettingInfo>
      </div>
      <slot name="remotes" />
    </section>

    <section v-if="config && activeMenu === 'integration'" class="settings-group">
      <div class="settings-group-heading">
        <div class="settings-group-title">
          <BookOpen :size="18" />
          <div>
            <h2>Help &amp; documentation</h2>
            <p>Inspect endpoint behaviour and test requests without leaving ix.print.</p>
          </div>
        </div>
        <SettingInfo title="Help & documentation">
          <p>
            The documentation is served by the local ix.print API, so examples use the same server port and security
            rules as real requests.
          </p>
          <ol>
            <li>Open the in-app guide to read the endpoint contract and parameters.</li>
            <li>Use the interactive forms to inspect printers, presets, queues and job status.</li>
            <li>Copy the address when another web app or computer needs to connect.</li>
          </ol>
        </SettingInfo>
      </div>
      <div class="card stack tight">
        <p class="hint">
          {{ config.lan_access ? "From another computer:" : "This computer only:" }}
          <code>http://{{ localIp }}:{{ config.server_port }}/docs</code>
        </p>
        <div class="row wrap">
          <button class="ghost small" @click="emit('docs', false)"><BookOpen :size="14" /> Open documentation</button>
          <button class="ghost small" @click="emit('docs', true)"><ExternalLink :size="14" /> Open in browser</button>
          <button class="ghost small" @click="emit('copy', `http://${localIp}:${config.server_port}/docs`)">
            Copy address
          </button>
        </div>
      </div>
    </section>

    <section v-if="config && activeMenu === 'local-data'" class="settings-group">
      <div class="settings-group-heading">
        <div class="settings-group-title">
          <FolderOpen :size="18" />
          <div>
            <h2>Local data</h2>
            <p>Where ix.print stores its settings, history and temporary files.</p>
          </div>
        </div>
        <SettingInfo title="Local data">
          <p>ix.print keeps everything it needs on this computer only — nothing is sent elsewhere.</p>
          <ol>
            <li>
              <code>config.json</code> holds the default printer, presets and token hashes (a backup is kept as
              <code>config.json.bak</code>).
            </li>
            <li><code>history/</code> holds one file per day of print jobs, pruned after the retention period.</li>
            <li><code>api-access-log.jsonl</code> records bounded HTTP request and response metadata without bodies or secrets.</li>
            <li><code>jobs/</code> holds temporary files while a job is printing; they are deleted right after.</li>
          </ol>
          <p class="info-note">Open the folder below to inspect or back up these files directly.</p>
        </SettingInfo>
      </div>
      <div class="card stack tight">
        <p class="hint local-data-path">
          <code>{{ dataFolderPath || "…" }}</code>
        </p>
        <div class="row wrap">
          <button class="ghost small" @click="openDataFolder"><FolderOpen :size="14" /> Open folder</button>
          <button class="ghost small" :disabled="!dataFolderPath" @click="emit('copy', dataFolderPath)">
            Copy path
          </button>
        </div>
      </div>
    </section>
    </div>
    </Transition>
  </section>
</template>

<style scoped>
.settings-page {
  gap: var(--section-gap);
  width: 100%;
  min-width: 0;
}

.settings-content {
  min-width: 0;
}

.settings-menu {
  display: grid;
  min-width: 0;
}

.settings-menu-item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  min-height: 64px;
  padding: 9px 4px;
  border: 0;
  border-bottom: 1px solid var(--line);
  border-radius: 0;
  background: transparent;
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.settings-menu-item:hover {
  background: var(--brand-soft);
}

.settings-menu-icon {
  flex: 0 0 auto;
  color: var(--brand);
}

.settings-menu-copy {
  display: grid;
  flex: 1;
  min-width: 0;
}

.settings-menu-copy strong {
  font-size: 14px;
  font-weight: 550;
  line-height: 20px;
}

.settings-menu-copy small {
  color: var(--muted);
  font-size: 12px;
  line-height: 18px;
  white-space: normal;
}

.settings-menu-arrow {
  flex: 0 0 auto;
  color: var(--muted);
}

.settings-config-error {
  display: grid;
  justify-items: start;
  gap: 8px;
  padding: 10px 12px;
  border-left: 2px solid var(--danger);
  border-radius: var(--r-sm);
  background: var(--danger-soft);
  color: var(--danger);
  overflow-wrap: anywhere;
}

.settings-config-error p {
  margin: 0;
}

.local-data-path {
  overflow-wrap: anywhere;
}

.settings-log-choices {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

.settings-log-choices button {
  min-width: 0;
  min-height: 44px;
  padding: 8px;
  border: 1px solid var(--border-control);
  border-radius: var(--r-md);
  background: var(--surface);
  color: var(--muted);
  font: inherit;
  font-weight: 500;
  overflow-wrap: anywhere;
  cursor: pointer;
}

.settings-log-choices button.active {
  border-color: var(--brand);
  background: var(--brand-soft);
  color: var(--text);
}

.settings-group {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: var(--stack-gap);
  min-width: 0;
}

.settings-group > * {
  min-width: 0;
}

.settings-group + .settings-group {
  padding-top: 4px;
  border-top: 1px solid var(--line);
}

.settings-group-heading {
  display: flex;
  align-items: flex-start;
  gap: var(--row-gap);
  padding: 0 2px;
}

.settings-group-title {
  display: flex;
  align-items: flex-start;
  gap: var(--row-gap);
  min-width: 0;
  color: var(--brand);
}

.settings-group-title h2 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  letter-spacing: -0.015em;
  color: var(--text);
}

.settings-group-title p {
  margin: 2px 0 0;
  font-size: 13px;
  line-height: 1.45;
  color: var(--muted);
}

.appearance-select {
  min-height: 44px;
}

@media (pointer: coarse) {
  .appearance-select {
    font-size: 16px;
    line-height: 24px;
  }
}

.settings-group-heading > .setting-info {
  margin-left: auto;
}
</style>
