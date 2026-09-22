<script setup lang="ts">
import { ref, watch } from "vue";
import { BookOpen, ExternalLink } from "lucide-vue-next";
import { api, errorText, type Config } from "../api";
import ApiTab from "./ApiTab.vue";
import RemotesPanel from "./RemotesPanel.vue";

const props = defineProps<{ config: Config; localIp: string }>();
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
</script>

<template>
	<section class="stack">
		<fieldset>
			<legend>Application bridge</legend>
			<label class="field" for="server-port">
				<span>Server port</span>
				<div class="row">
					<input id="server-port" v-model="port" class="port-input grow" inputmode="numeric" pattern="[0-9]*" autocomplete="off" />
					<button class="primary small" type="button" :disabled="savingPort" @click="savePort">{{ savingPort ? "Saving…" : "Save port" }}</button>
				</div>
			</label>
			<p class="hint">Port used by web apps and the API documentation. Use a whole number from 1024 to 65535. The bridge restarts after a change.</p>
		</fieldset>

		<fieldset>
			<legend>REST API documentation</legend>
			<p class="hint">
				Endpoint guide and forms for trying the API directly. From another computer, open
				<code>http://{{ localIp }}:{{ config.server_port }}/docs</code>.
			</p>
			<div class="row wrap">
				<button class="ghost small" @click="emit('docs', false)"><BookOpen :size="14" /> Open documentation</button>
				<button class="ghost small" @click="emit('docs', true)"><ExternalLink :size="14" /> Open in browser</button>
				<button class="ghost small" @click="emit('copy', `http://${localIp}:${config.server_port}/docs`)">Copy address</button>
			</div>
		</fieldset>

		<h3 class="section-title">API security (token)</h3>
		<ApiTab
			:config="config"
			:local-ip="localIp"
			@config="emit('config', $event)"
			@notify="(m, e) => emit('notify', m, e)"
			@copy="emit('copy', $event)"
		/>

		<h3 class="section-title">Other ix.print computers</h3>
		<RemotesPanel @notify="(m, e) => emit('notify', m, e)" @printers-changed="emit('printers-changed')" />
	</section>
</template>
