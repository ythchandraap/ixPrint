<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { BookOpen, ExternalLink, FolderOpen, Network, Server, ShieldCheck } from "lucide-vue-next";
import { api, errorText, type Config } from "../api";
import ApiTab from "./ApiTab.vue";
import ErrorLogPanel from "./ErrorLogPanel.vue";
import RemotesPanel from "./RemotesPanel.vue";
import SettingInfo from "./SettingInfo.vue";

const props = defineProps<{ config: Config; localIp: string; os: string }>();
const emit = defineEmits<{
	(e: "config", config: Config): void;
	(e: "notify", message: string, error?: boolean): void;
	(e: "copy", text: string): void;
	(e: "docs", external: boolean): void;
	(e: "printers-changed"): void;
}>();

const port = ref("");
const savingPort = ref(false);
watch(
	() => props.config.server_port,
	(value) => (port.value = String(value)),
	{ immediate: true },
);

async function savePort() {
	const value = Number(port.value.trim());
	if (!Number.isInteger(value) || value < 1024 || value > 65535) {
		emit("notify", "Server port must be a whole number between 1024 and 65535", true);
		return;
	}
	savingPort.value = true;
	try {
		emit("config", await api.setServerPort(value));
		emit("notify", props.config.bridge_enabled ? `Server port changed to ${value}; the application bridge is restarting` : `Server port changed to ${value}`);
	} catch (e) {
		emit("notify", errorText(e), true);
	} finally {
		savingPort.value = false;
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
		<ErrorLogPanel :show-system-logs="props.os === 'windows'" @copy="emit('copy', $event)" @notify="(m, e) => emit('notify', m, e)" />

		<section class="settings-group">
			<div class="settings-group-heading">
				<div class="settings-group-title">
					<Server :size="18" />
					<div><h2>Bridge &amp; connection</h2><p>Control the local HTTP service used by web apps and API tools.</p></div>
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

			<fieldset>
				<legend>Server port</legend>
			<label class="field" for="server-port">
				<span>Port used by web apps and the API</span>
				<div class="row">
					<input id="server-port" v-model="port" class="port-input grow" inputmode="numeric" pattern="[0-9]*" autocomplete="off" />
					<button class="primary small" type="button" :disabled="savingPort" @click="savePort">{{ savingPort ? "Saving…" : "Save port" }}</button>
				</div>
			</label>
			<p class="hint">Restarts the bridge after a change.</p>
			</fieldset>
		</section>

		<section class="settings-group">
			<div class="settings-group-heading">
				<div class="settings-group-title">
					<ShieldCheck :size="18" />
					<div><h2>API access &amp; security</h2><p>Protect print requests and limit which browser apps may connect.</p></div>
				</div>
				<SettingInfo title="API access & security">
					<p>This group controls who can call the local API. It does not change direct OS printer permissions.</p>
					<ol>
						<li>For browser requests, ix.print checks the web app origin against Allowed websites.</li>
						<li>It checks the API token when Require a token is enabled.</li>
						<li>Only then can the request list printers, inspect queues or submit a print job.</li>
					</ol>
					<p>Allowed websites only limits browser (JavaScript fetch) calls — server-side apps, curl and other non-browser clients are not restricted by it.</p>
					<p class="info-note">Tokens are stored as hashes. The full token is shown only once when created.</p>
				</SettingInfo>
			</div>
			<ApiTab
				:config="config"
				:local-ip="localIp"
				@config="emit('config', $event)"
				@notify="(m, e) => emit('notify', m, e)"
				@copy="emit('copy', $event)"
			/>
		</section>

		<section class="settings-group">
			<div class="settings-group-heading">
				<div class="settings-group-title">
					<Network :size="18" />
					<div><h2>Remote printers</h2><p>Use printers connected to another ix.print computer on your network.</p></div>
				</div>
				<SettingInfo title="Remote printers">
					<p>A remote computer is another ix.print instance that you explicitly add with its address and API token.</p>
					<ol>
						<li>ix.print tests the address and token before saving the remote.</li>
						<li>Its local printers appear as <code>Remote name / Printer name</code>.</li>
						<li>A selected job is forwarded directly to that computer, which prints it and reports status back.</li>
					</ol>
					<p class="info-note">Remote forwarding is limited to one hop. Use HTTPS or a trusted LAN when sending tokens and print data.</p>
				</SettingInfo>
			</div>
			<RemotesPanel @notify="(m, e) => emit('notify', m, e)" @printers-changed="emit('printers-changed')" />
		</section>

		<section class="settings-group">
			<div class="settings-group-heading">
				<div class="settings-group-title">
					<BookOpen :size="18" />
					<div><h2>Help &amp; documentation</h2><p>Inspect endpoint behaviour and test requests without leaving ix.print.</p></div>
				</div>
				<SettingInfo title="Help & documentation">
					<p>The documentation is served by the local ix.print API, so examples use the same server port and security rules as real requests.</p>
					<ol>
						<li>Open the in-app guide to read the endpoint contract and parameters.</li>
						<li>Use the interactive forms to inspect printers, presets, queues and job status.</li>
						<li>Copy the address when another web app or computer needs to connect.</li>
					</ol>
				</SettingInfo>
			</div>
			<div class="card stack tight">
				<p class="hint">From another computer: <code>http://{{ localIp }}:{{ config.server_port }}/docs</code></p>
				<div class="row wrap">
					<button class="ghost small" @click="emit('docs', false)"><BookOpen :size="14" /> Open documentation</button>
					<button class="ghost small" @click="emit('docs', true)"><ExternalLink :size="14" /> Open in browser</button>
					<button class="ghost small" @click="emit('copy', `http://${localIp}:${config.server_port}/docs`)">Copy address</button>
				</div>
			</div>
		</section>

		<section class="settings-group">
			<div class="settings-group-heading">
				<div class="settings-group-title">
					<FolderOpen :size="18" />
					<div><h2>Local data</h2><p>Where ix.print stores its settings, history and temporary files.</p></div>
				</div>
				<SettingInfo title="Local data">
					<p>ix.print keeps everything it needs on this computer only — nothing is sent elsewhere.</p>
					<ol>
						<li><code>config.json</code> holds the default printer, presets and token hashes (a backup is kept as <code>config.json.bak</code>).</li>
						<li><code>history/</code> holds one file per day of print jobs, pruned after the retention period.</li>
						<li><code>jobs/</code> holds temporary files while a job is printing; they are deleted right after.</li>
					</ol>
					<p class="info-note">Open the folder below to inspect or back up these files directly.</p>
				</SettingInfo>
			</div>
			<div class="card stack tight">
				<p class="hint"><code>{{ dataFolderPath || "…" }}</code></p>
				<div class="row wrap">
					<button class="ghost small" @click="openDataFolder"><FolderOpen :size="14" /> Open folder</button>
					<button class="ghost small" :disabled="!dataFolderPath" @click="emit('copy', dataFolderPath)">Copy path</button>
				</div>
			</div>
		</section>
	</section>
</template>

<style scoped>
.settings-page {
	gap: 20px;
}

.settings-group {
	display: grid;
	gap: 10px;
}

.settings-group + .settings-group {
	padding-top: 4px;
	border-top: 1px solid var(--line);
}

.settings-group-heading {
	display: flex;
	align-items: flex-start;
	gap: 12px;
	padding: 0 2px;
}

.settings-group-title {
	display: flex;
	align-items: flex-start;
	gap: 9px;
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
	font-size: 12px;
	line-height: 1.45;
	color: var(--muted);
}

.settings-group-heading > :last-child {
	margin-left: auto;
}
</style>
