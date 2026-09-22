<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { Copy, Globe, KeyRound, Plus, Trash2 } from "lucide-vue-next";
import EmptyState from "./EmptyState.vue";
import { api, errorText, type Config, type TokenUse } from "../api";

const props = defineProps<{ config: Config; localIp: string }>();
const emit = defineEmits<{
	(e: "config", config: Config): void;
	(e: "notify", message: string, error?: boolean): void;
	(e: "copy", text: string): void;
}>();

const newName = ref("");
const creating = ref(false);
/** Token just created: shown once, until dismissed. */
const fresh = ref<{ name: string; token: string } | null>(null);
const usage = ref<Record<string, TokenUse>>({});
let timer: number | undefined;

async function loadUsage() {
	try {
		usage.value = await api.tokenUsage();
	} catch {
		// keep the last values
	}
}
onMounted(() => {
	loadUsage();
	timer = window.setInterval(loadUsage, 5000);
});
onBeforeUnmount(() => clearInterval(timer));

async function create() {
	creating.value = true;
	try {
		const { token, config } = await api.createToken(newName.value);
		fresh.value = { name: newName.value.trim(), token };
		newName.value = "";
		emit("config", config);
	} catch (e) {
		emit("notify", errorText(e), true);
	} finally {
		creating.value = false;
	}
}

async function revoke(id: string, name: string) {
	const ok = await confirm(`Revoke token "${name}"? Applications using it will no longer be able to print.`, {
		title: "ix.print",
		kind: "warning",
	});
	if (!ok) return;
	try {
		emit("config", await api.revokeToken(id));
		emit("notify", `Token "${name}" revoked`);
	} catch (e) {
		emit("notify", errorText(e), true);
	}
}

async function toggleRequired(event: Event) {
	const input = event.target as HTMLInputElement;
	try {
		emit("config", await api.setRequireToken(input.checked));
		emit("notify", input.checked ? "Token is now required" : "Token is no longer required");
	} catch (e) {
		input.checked = props.config.require_token;
		emit("notify", errorText(e), true);
	}
}

const newOrigin = ref("");
const anyOrigin = () => props.config.allowed_origins.includes("*");

async function saveOrigins(origins: string[], message: string) {
	try {
		emit("config", await api.setAllowedOrigins(origins));
		emit("notify", message);
		return true;
	} catch (e) {
		emit("notify", errorText(e), true);
		return false;
	}
}

async function addOrigin() {
	const origin = newOrigin.value.trim();
	if (await saveOrigins([...props.config.allowed_origins, origin], `${origin} allowed`)) newOrigin.value = "";
}

async function removeOrigin(origin: string) {
	const label = origin === "*" ? "all websites" : origin;
	const ok = await confirm(`Remove permission for ${label}? Web apps there will no longer be able to call the API from a browser.`, {
		title: "ix.print",
		kind: "warning",
	});
	if (ok) await saveOrigins(props.config.allowed_origins.filter((o) => o !== origin), `Permission for ${label} removed`);
}

const when = (iso: string) => new Date(iso).toLocaleString("en-GB", { dateStyle: "medium", timeStyle: "short" });
</script>

<template>
	<section class="stack">
		<div :class="['card', 'stack', 'tight', config.require_token ? 'secure' : 'open']">
			<label class="row check">
				<input type="checkbox" :checked="config.require_token" @change="toggleRequired" />
				<strong class="grow">Require a token to access the API</strong>
			</label>
			<p v-if="config.require_token && !config.tokens.length" class="warn-text small-text">
				No tokens yet: all API requests are rejected. Create a token for each application that prints.
			</p>
			<p v-else-if="config.require_token" class="hint">
				Only applications with a valid token can print or read printers and queues through port {{ config.server_port }}.
			</p>
			<p v-else class="warn-text small-text">
				⚠ Not required: anyone on the network who knows {{ localIp }}:{{ config.server_port }} can print.
				Enable this again after a token is configured in every web app.
			</p>
		</div>

		<div v-if="fresh" class="card stack tight fresh">
			<strong>Token "{{ fresh.name }}" created</strong>
			<p class="small-text">Copy it now. This token is <b>shown only once</b>; ix.print stores only its hash.</p>
			<div class="row">
				<code class="grow token">{{ fresh.token }}</code>
				<button class="icon" title="Copy token" @click="emit('copy', fresh.token)"><Copy :size="16" /></button>
			</div>
			<div class="row end">
				<button class="ghost small" @click="fresh = null">Already copied</button>
			</div>
		</div>

		<form class="row" @submit.prevent="create">
			<input v-model="newName" class="grow" maxlength="64" required placeholder="Application name, e.g. ERP or till" />
			<button class="primary small" type="submit" :disabled="creating"><KeyRound :size="14" /> Create token</button>
		</form>

		<EmptyState
			v-if="!config.tokens.length"
			:icon="KeyRound"
			title="No tokens yet"
			text="Create one token for each application that prints (such as a business app, till or label system), then enable “Require a token”."
		/>
		<div v-for="t in config.tokens" :key="t.id" class="card row">
			<div class="grow min0">
				<div class="row tight">
					<strong class="truncate">{{ t.name }}</strong>
					<code class="meta">{{ t.hint }}</code>
				</div>
				<div class="meta">Created {{ when(t.created_at) }}</div>
				<div class="meta">
					<template v-if="usage[t.id]">Last used {{ when(usage[t.id].time) }} by {{ usage[t.id].client ?? "?" }}</template>
					<template v-else>Not used since ix.print started</template>
				</div>
			</div>
			<button class="icon danger" title="Revoke token" @click="revoke(t.id, t.name)"><Trash2 :size="16" /></button>
		</div>

		<fieldset class="stack tight">
			<legend>Allowed websites</legend>
			<p class="small-text">
				Web apps calling the API <b>from a browser</b> can only do so when their address is listed here. Server applications,
				curl and other programs are not affected.
			</p>
			<p v-if="anyOrigin()" class="warn-text small-text">
				⚠ <code>*</code> = every website is allowed, including unrelated sites opened on a computer on the same network.
				Replace it with your web app address, then remove <code>*</code>.
			</p>
			<p v-else-if="!config.allowed_origins.length" class="hint">
				None yet: no website can call the API from a browser.
			</p>
			<div v-for="o in config.allowed_origins" :key="o" class="card row">
				<Globe :size="16" />
				<code class="grow truncate">{{ o === "*" ? "* (all websites)" : o }}</code>
				<button class="icon danger" title="Remove permission" @click="removeOrigin(o)"><Trash2 :size="16" /></button>
			</div>
			<form class="row" @submit.prevent="addOrigin">
					<input v-model="newOrigin" class="grow" required placeholder="https://app.example.com" />
				<button class="primary small" type="submit"><Plus :size="14" /> Allow</button>
			</form>
			<p class="hint">Enter the scheme, host and port only (no path), exactly as shown in the web app's address bar.</p>
		</fieldset>

		<fieldset>
			<legend>How to use</legend>
			<p class="small-text">Send the token with every request (choose one):</p>
			<pre class="settings">Authorization: Bearer fxp_…
X-Api-Token: fxp_…
?token=fxp_…</pre>
			<p class="hint">Avoid <code>?token=</code> where possible: it is recorded in logs and browser history.</p>
		</fieldset>
	</section>
</template>

<style scoped>
.secure {
	border-color: var(--ok);
}
.open {
	border-color: var(--warn);
}
.fresh {
	border-color: var(--brand);
	background: var(--brand-pale);
	color: var(--text);
}
.token {
	font-family: ui-monospace, Menlo, Consolas, monospace;
	font-size: 12px;
	word-break: break-all;
	background: var(--surface);
	padding: 6px 8px;
	border-radius: 6px;
	user-select: all;
}
</style>
