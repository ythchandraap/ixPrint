<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { ArrowLeft, Copy, MonitorX, Printer, Settings } from "lucide-vue-next";
import { api, errorText, isInApp, type Config, type PrinterView } from "./api";
import PrintersTab from "@/features/printers/PrintersTab.vue";
import PresetsTab from "@/features/presets/PresetsTab.vue";
import HistoryTab from "./components/HistoryTab.vue";
import PrintTab from "@/features/print/PrintTab.vue";
import ErrorLogPanel from "./components/ErrorLogPanel.vue";
import SettingsView from "./components/SettingsView.vue";
import EmptyState from "./components/EmptyState.vue";
import OnboardingDialog from "./components/OnboardingDialog.vue";

/** Product name, in one place so a rename touches a single line. */
const PRODUCT = "ix.print";
/** Parent brand; the product remains the primary task identifier. */
const BRAND = "itz.ixxyyzz";
const DEFAULT_PORT = 18032;
const ONBOARDING_KEY = "ixprint.onboarding.v1";

const inApp = isInApp();
const version = ref("");
/** HTTP server state for the status dot. */
const serverState = ref<"loading" | "ok" | "error" | "off">("loading");
const serverError = ref("");
const bridgeEnabled = ref(true);
const bridgeChanging = ref(false);
const localIp = ref("");
/** loading until the first answer; none = no network connection. */
const ipState = ref<"loading" | "ok" | "none">("loading");
const configError = ref("");
const os = ref("");
const engineError = ref("");
const printers = ref<PrinterView[]>([]);
const config = ref<Config | null>(null);
const selectedPrinter = ref("");
const tab = ref<"printers" | "presets" | "history">("printers");
const printPageOpen = ref(false);
const requestedPreset = ref("");
const tabLabel = computed(() => ({ printers: "Printers", presets: "Presets", history: "History" })[tab.value]);
/** Settings (API tokens, API docs) replace the tabs while open. */
const settingsOpen = ref(false);
const onboardingOpen = ref(false);
const toast = ref<{ text: string; error: boolean } | null>(null);
const timers: number[] = [];
let toastTimer: number | undefined;
let unlistenTray: (() => void) | undefined;
let unlistenBridge: (() => void) | undefined;

function notify(text: string, error = false) {
	toast.value = { text, error };
	clearTimeout(toastTimer);
	toastTimer = window.setTimeout(() => (toast.value = null), error ? 6000 : 3000);
	if (error && inApp) void api.recordError("ui-notification", text).catch(() => {});
}

function needsOnboarding() {
	try {
		return localStorage.getItem(ONBOARDING_KEY) !== "accepted";
	} catch {
		return true;
	}
}

function completeOnboarding() {
	try {
		localStorage.setItem(ONBOARDING_KEY, "accepted");
	} catch {
		// Keep the acknowledgement for this session when storage is unavailable.
	}
	onboardingOpen.value = false;
}

const refreshing = ref(false);
const printersLoaded = ref(false);

/** Reloads the printer list and announces printers that appeared or disappeared. */
async function refreshPrinters(manual = false) {
	if (manual) refreshing.value = true;
	try {
		const next = await api.printers();
		const before = new Set(printers.value.map((p) => p.name));
		const after = new Set(next.map((p) => p.name));
		const added = next.filter((p) => !before.has(p.name)).map((p) => p.name);
		const removed = [...before].filter((name) => !after.has(name));
		printers.value = next;

		if (selectedPrinter.value && !after.has(selectedPrinter.value)) selectedPrinter.value = "";
		if (printersLoaded.value && added.length) notify(`New printer detected: ${added.join(", ")}`);
		else if (printersLoaded.value && removed.length) notify(`Printer removed: ${removed.join(", ")}`, true);
		else if (manual) notify(`${next.length} printer${next.length === 1 ? "" : "s"} found`);
		printersLoaded.value = true;
	} catch (e) {
		// keep the last list; the next poll retries
		if (manual) notify(errorText(e), true);
	} finally {
		refreshing.value = false;
	}
}

async function refreshIp() {
	try {
		const ip = await api.localIp();
		localIp.value = ip ?? "";
		ipState.value = ip ? "ok" : "none";
	} catch {
		ipState.value = "none";
	}
}

async function openDocs(external = false) {
	try {
		await api.openDocs(external);
	} catch (e) {
		notify(errorText(e), true);
	}
}

const OS_LABEL: Record<string, string> = { macos: "macOS", windows: "Windows", linux: "Linux" };
const osLabel = computed(() => OS_LABEL[os.value] ?? os.value);
const bridgeHost = computed(() => (config.value?.lan_access === false ? "127.0.0.1" : localIp.value || "localhost"));
const address = computed(() => {
	const port = config.value?.server_port ?? DEFAULT_PORT;
	return `${bridgeHost.value}:${port}`;
});

function copy(text: string) {
	navigator.clipboard.writeText(text).then(() => notify(`Copied: ${text}`));
}

async function toggleDefaultPrinter(name: string) {
	try {
		const next = config.value?.default_printer === name ? null : name;
		config.value = await api.setDefaultPrinter(next);
		refreshPrinters();
	} catch (e) {
		notify(errorText(e), true);
	}
}

/** Pages per test print; remembered on this computer. */
const MAX_TEST_PAGES = 50;
function loadTestPages(): number {
	try {
		const n = Number(localStorage.getItem("ixprint.testPages"));
		return n >= 1 && n <= MAX_TEST_PAGES ? n : 1;
	} catch {
		return 1;
	}
}
const testPages = ref(loadTestPages());
function setTestPages(n: number) {
	testPages.value = Math.min(MAX_TEST_PAGES, Math.max(1, Math.round(n) || 1));
	try {
		localStorage.setItem("ixprint.testPages", String(testPages.value));
	} catch {
		// storage unavailable; keep it for this session only
	}
}

function openPrintPage(preset = "") {
	requestedPreset.value = preset;
	settingsOpen.value = false;
	printPageOpen.value = true;
}

function toggleSettings() {
	if (settingsOpen.value) {
		settingsOpen.value = false;
	} else {
		settingsOpen.value = true;
		printPageOpen.value = false;
	}
}

function closePrintPage() {
	printPageOpen.value = false;
}

function selectTab(name: "printers" | "presets" | "history") {
	printPageOpen.value = false;
	tab.value = name;
}

function reload() {
	window.location.reload();
}

async function checkServerStatus() {
	if (!bridgeEnabled.value) {
		serverState.value = "off";
		serverError.value = "";
		return;
	}
	try {
		await api.serverStatus();
		serverState.value = "ok";
		serverError.value = "";
	} catch (e) {
		serverState.value = "error";
		serverError.value = errorText(e);
	}
}

async function toggleBridge() {
	const next = !bridgeEnabled.value;
	bridgeChanging.value = true;
	try {
		config.value = await api.setBridgeEnabled(next);
		bridgeEnabled.value = next;
		serverError.value = "";
		if (next) {
			serverState.value = "loading";
			window.setTimeout(checkServerStatus, 500);
			notify("Application bridge enabled");
		} else {
			serverState.value = "off";
			notify("Application bridge disabled");
		}
	} catch (e) {
		notify(errorText(e), true);
	} finally {
		bridgeChanging.value = false;
	}
}

function setConfig(next: Config) {
	const bridgeChanged =
		config.value?.server_port !== next.server_port || config.value?.lan_access !== next.lan_access;
	config.value = next;
	refreshPrinters();
	if (bridgeChanged && next.bridge_enabled) {
		serverState.value = "loading";
		serverError.value = "";
		window.setTimeout(checkServerStatus, 500);
	}
}

onMounted(async () => {
	if (!inApp) return; // opened in a normal browser: nothing to talk to
	onboardingOpen.value = needsOnboarding();
	unlistenTray = await listen<string>("tray-navigate", (event) => {
		if (event.payload === "errors") {
			settingsOpen.value = true;
			printPageOpen.value = false;
			nextTick(() => document.getElementById("error-logs")?.scrollIntoView({ block: "start", behavior: "smooth" }));
		} else {
			settingsOpen.value = false;
			printPageOpen.value = false;
			tab.value = event.payload === "history" ? "history" : "printers";
		}
	});
	unlistenBridge = await listen<boolean>("bridge-changed", (event) => {
		bridgeEnabled.value = event.payload;
		if (config.value) config.value = { ...config.value, bridge_enabled: event.payload };
		serverState.value = event.payload ? "loading" : "off";
		if (event.payload) window.setTimeout(checkServerStatus, 300);
	});
	// Independent loads: one failing must not leave the rest of the screen empty.
	api.os().then((v) => (os.value = v)).catch(() => {});
	api.engineStatus().catch((e) => (engineError.value = errorText(e)));
	getVersion().then((v) => (version.value = v)).catch(() => {});
	api.bridgeEnabled()
		.then((enabled) => {
			bridgeEnabled.value = enabled;
			serverState.value = enabled ? "loading" : "off";
			window.setTimeout(checkServerStatus, enabled ? 800 : 0);
		})
		.catch(() => window.setTimeout(checkServerStatus, 800));
	refreshIp();
	refreshPrinters();
	try {
		config.value = await api.config();
	} catch (e) {
		configError.value = errorText(e);
	}
	timers.push(window.setInterval(() => refreshPrinters(), 3000), window.setInterval(refreshIp, 10000));
});

onBeforeUnmount(() => {
	timers.forEach(clearInterval);
	unlistenTray?.();
	unlistenBridge?.();
});
</script>

<template>
	<div class="app">
		<div class="top">
			<header class="appbar">
				<div class="bar">
					<span class="wordmark" :aria-label="PRODUCT">{{ PRODUCT }}</span>
					<span class="grow"></span>
					<button
						:class="['bar-btn', { on: printPageOpen }]"
						:aria-pressed="printPageOpen"
						:disabled="!inApp"
						title="Open test print options"
						@click="openPrintPage()"
					>
						<Printer :size="14" /> Test print
					</button>
					<button
						:class="['bar-icon', { on: settingsOpen }]"
						:aria-pressed="settingsOpen"
						aria-label="Settings: API & documentation"
						title="Settings: API & documentation"
						:disabled="!inApp"
						@click="toggleSettings()"
					>
						<Settings :size="16" />
					</button>
				</div>

				<div class="statusline">
					<template v-if="!inApp">
						<span class="status"><i class="dot idle"></i>NOT IN APP</span>
					</template>
					<template v-else>
						<button
							class="bridge-toggle"
							:aria-pressed="bridgeEnabled"
							:disabled="bridgeChanging"
							:title="bridgeEnabled ? 'Disable the bridge for web apps' : 'Enable the bridge for web apps'"
							@click="toggleBridge"
						>
							<i :class="['dot', !bridgeEnabled ? 'idle' : serverState === 'ok' ? 'on' : serverState === 'error' ? 'fail' : 'idle']"></i>
							{{ !bridgeEnabled ? "BRIDGE OFF" : serverState === "ok" ? "BRIDGE ON" : serverState === "error" ? "BRIDGE FAILED" : "STARTING" }}
						</button>
						<span class="sep" aria-hidden="true">/</span>
						<span v-if="ipState === 'loading' && config?.lan_access !== false" class="skeleton">LOADING ADDRESS</span>
						<button
							v-else
							class="addr"
							:title="config?.lan_access === false ? 'Copy the local bridge address' : ipState === 'ok' ? 'Copy the address for a web app' : 'Not connected to a network: only accessible from this computer'"
							@click="copy(address)"
						>
							{{ address }}<Copy :size="11" />
						</button>
						<span v-if="ipState === 'none' && config?.lan_access !== false" class="warn-note">OFFLINE</span>
						<template v-if="config">
							<span class="sep" aria-hidden="true">/</span>
							<span :class="{ 'warn-note': !config.require_token }">
								{{ config.require_token ? "TOKEN REQUIRED" : "TOKEN OPTIONAL" }}
							</span>
						</template>
					</template>
				</div>
			</header>
			<p v-if="serverState === 'error'" class="banner">{{ serverError }}. Close any other application using the configured port, then restart {{ PRODUCT }}.</p>

			<p v-if="engineError" class="banner">PDF engine unavailable: {{ engineError }}</p>
			<nav v-if="settingsOpen" class="tabs settings-bar">
				<button class="icon" title="Back" @click="settingsOpen = false"><ArrowLeft :size="18" /></button>
				<strong>Settings</strong>
			</nav>
			<nav v-else class="tabs">
				<button :class="{ on: !printPageOpen && tab === 'printers' }" @click="selectTab('printers')">Printers</button>
				<button :class="{ on: !printPageOpen && tab === 'presets' }" @click="selectTab('presets')">Presets</button>
				<button :class="{ on: !printPageOpen && tab === 'history' }" @click="selectTab('history')">History</button>
			</nav>
		</div>

		<main v-if="!inApp">
			<EmptyState
				:icon="MonitorX"
				title="Open this in the ix.print app"
				text="This is the ix.print desktop app and cannot be used directly in a browser. To try the REST API from a browser, open http://<computer-ip>:<server-port>/docs."
			/>
		</main>
		<main v-else-if="configError">
			<EmptyState :icon="MonitorX" tone="warn" title="Settings could not be loaded" :text="configError">
				<button class="primary small" @click="reload">Try again</button>
			</EmptyState>
			<ErrorLogPanel v-if="settingsOpen" :show-system-logs="os === 'windows'" @copy="copy" @notify="notify" />
		</main>
		<main v-else-if="!config">
			<p class="muted center-text">Loading…</p>
		</main>
		<main v-else>
			<SettingsView
				v-if="settingsOpen"
				:config="config"
				:local-ip="bridgeHost"
				:os="os"
				@config="setConfig"
				@notify="notify"
				@copy="copy"
				@docs="openDocs"
				@printers-changed="refreshPrinters"
			/>
			<PrintTab
				v-else-if="printPageOpen"
				:config="config"
				:printers="printers"
				:printers-loaded="printersLoaded"
				:selected-printer="selectedPrinter"
				:test-pages="testPages"
				:requested-preset="requestedPreset"
				:return-label="tabLabel"
				@back="closePrintPage"
				@presets="selectTab('presets')"
				@printers="selectTab('printers')"
				@select-printer="selectedPrinter = $event"
				@test-pages="setTestPages"
			/>
			<PrintersTab
				v-else-if="tab === 'printers'"
				:printers="printers"
				:config="config"
					:selected="selectedPrinter"
					:refreshing="refreshing"
					:loaded="printersLoaded"
					:os="os"
					@refresh="refreshPrinters(true)"
				@reload="refreshPrinters()"
				@select="selectedPrinter = $event"
				@toggle-default="toggleDefaultPrinter"
				@copy="copy"
				@notify="notify"
				@config="setConfig"
			/>
			<PresetsTab
				v-else-if="tab === 'presets'"
				:config="config"
				:printers="printers"
				:test-pages="testPages"
				:local-ip="bridgeHost"
				:server-port="config.server_port"
				@config="setConfig"
				@notify="notify"
				@open-print="openPrintPage"
			/>
			<HistoryTab v-else :config="config" @config="setConfig" @notify="notify" />
		</main>

		<footer class="colophon">
			<span>{{ PRODUCT }}{{ version ? ` ${version}` : "" }}{{ osLabel ? ` · ${osLabel}` : "" }}</span>
			<span class="grow"></span>
			<span>IX Systems / {{ BRAND }}</span>
		</footer>

		<OnboardingDialog v-if="onboardingOpen" @complete="completeOnboarding" />

		<div v-if="toast" :class="['toast', { error: toast.error }]" role="status" @click="toast = null">{{ toast.text }}</div>
	</div>
</template>

<style>
/* ---------------------------------------------------------------------------
 * itz.ixxyyzz — Quiet Technology tokens.
 * Primitives are the brand palette; components only use the semantic tokens.
 * ------------------------------------------------------------------------- */
@font-face {
	font-family: "Geist";
	src: url("./assets/fonts/geist/Geist-Variable.woff2") format("woff2");
	font-weight: 100 900;
	font-display: swap;
}
@font-face {
	font-family: "Geist Mono";
	src: url("./assets/fonts/geist/GeistMono-Variable.woff2") format("woff2");
	font-weight: 100 900;
	font-display: swap;
}

:root {
	/* primitives */
	--obsidian: #0b0c0e;
	--graphite: #17191d;
	--carbon: #22252a;
	--ivory: #f2f1ed;
	--steel: #9ca3ad;
	--border-dark: #292c31;
	--cobalt: #536dfe;
	/* #536DFE with white text is ~4:1 (below WCAG AA): fills and small text use a deeper step. */
	--cobalt-deep: #3a54e0;
	--cobalt-light: #8a9bff;

	/* semantic — light */
	--bg: var(--ivory);
	--surface: #fbfbf9;
	--text: var(--obsidian);
	--muted: #5e646e;
	--line: #dad8d2;
	--accent: var(--cobalt);
	--brand: var(--cobalt-deep); /* accent as text / lines */
	--brand-fill: var(--cobalt-deep); /* accent as a fill behind white text */
	--brand-pale: #e6eafe; /* selected / hover backgrounds */
	--brand-ink: var(--cobalt-deep);
	--brand-soft: #e6eafe;
	--control-height: 36px;
	--ok: #1f7a4d;
	--ok-soft: #e3f1e9;
	--warn: #9a5b00;
	--danger: #c2382b;
	--danger-soft: #f9e6e3;

	/* type, radius, motion */
	--font: "Geist", "Inter", -apple-system, "Segoe UI", Helvetica, Arial, sans-serif;
	--font-mono: "Geist Mono", "IBM Plex Mono", ui-monospace, Menlo, Consolas, monospace;
	--r-sm: 4px;
	--r-md: 6px;
	--r-lg: 8px;
	--ease: cubic-bezier(0.2, 0, 0, 1);

	font-family: var(--font);
	font-size: 15px;
	line-height: 1.5;
	color: var(--text);
	background: var(--bg);
	-webkit-font-smoothing: antialiased;
	font-feature-settings: "ss01", "cv11";
}

@media (prefers-color-scheme: dark) {
	:root {
		--bg: var(--obsidian);
		--surface: var(--graphite);
		--text: var(--ivory);
		--muted: var(--steel);
		--line: var(--border-dark);
		--brand: var(--cobalt-light);
		--brand-pale: #1b2140;
		--brand-ink: var(--cobalt-light);
		--brand-soft: #1b2140;
		--ok: #5cc28e;
		--ok-soft: #142a1f;
		--warn: #e0a84a;
		--danger: #f07a6c;
		--danger-soft: #2e1714;
	}
}

* {
	box-sizing: border-box;
}
body {
	margin: 0;
	background: var(--bg);
}
:focus-visible {
	outline: 2px solid var(--accent);
	outline-offset: 1px;
}

.app {
	max-width: 560px;
	margin: 0 auto;
	min-height: 100vh;
	display: flex;
	flex-direction: column;
}

/* App bar, banners and tabs stay in view while the content scrolls. */
.top {
	position: sticky;
	top: 0;
	z-index: 10;
	background: var(--bg);
}

/* ---- app bar: the brand's dark surface, compact, information first ------ */
.appbar {
	background: var(--obsidian);
	color: var(--ivory);
	border-bottom: 1px solid var(--border-dark);
}
.appbar .bar {
	display: flex;
	align-items: center;
	gap: 8px;
	height: 52px;
	padding: 0 16px;
}
.wordmark {
	font-size: 17px;
	font-weight: 500;
	letter-spacing: -0.01em;
}
.bar-btn,
.bar-icon {
	font: inherit;
	font-size: 13px;
	font-weight: 500;
	color: var(--ivory);
	background: transparent;
	border: 1px solid var(--border-dark);
	border-radius: var(--r-md);
	height: 32px;
	display: inline-flex;
	align-items: center;
	gap: 6px;
	cursor: pointer;
	transition: background 150ms var(--ease), border-color 150ms var(--ease);
}
.bar-btn {
	padding: 0 12px;
}
.bar-icon {
	width: 32px;
	justify-content: center;
}
.bar-btn:hover:not(:disabled),
.bar-icon:hover:not(:disabled) {
	background: var(--carbon);
}
.bar-icon.on {
	border-color: var(--cobalt);
	color: var(--cobalt-light);
}
.bar-btn.on {
	border-color: var(--cobalt);
	color: var(--cobalt-light);
}
.statusline {
	display: flex;
	align-items: center;
	gap: 8px;
	min-height: 32px;
	padding: 0 16px 6px;
	font-family: var(--font-mono);
	font-size: 11px;
	letter-spacing: 0.02em;
	color: var(--steel);
	white-space: nowrap;
	overflow: hidden;
}
.statusline .status,
.statusline .bridge-toggle {
	display: inline-flex;
	align-items: center;
	gap: 6px;
	font: inherit;
	font-size: inherit;
	letter-spacing: inherit;
	white-space: nowrap;
}
.statusline .status {
	color: var(--ivory);
}
.statusline .bridge-toggle {
	min-height: 24px;
	padding: 3px 7px;
	color: var(--ivory);
	background: var(--carbon);
	border: 1px solid var(--border-dark);
	border-radius: var(--r-sm);
	cursor: pointer;
}
.statusline .bridge-toggle:hover:not(:disabled) {
	border-color: var(--cobalt);
}
.statusline .bridge-toggle:disabled {
	cursor: wait;
	opacity: 0.7;
}
.statusline .sep {
	color: var(--border-dark);
}
.statusline .addr {
	font: inherit;
	color: var(--ivory);
	background: none;
	border: none;
	padding: 2px 0;
	display: inline-flex;
	align-items: center;
	gap: 6px;
	cursor: pointer;
}
.statusline .addr:hover {
	color: var(--cobalt-light);
}
.statusline .warn-note {
	color: #e0a84a;
}

/* The dot: the brand's status primitive. */
.dot {
	width: 7px;
	height: 7px;
	border-radius: 50%;
	flex: none;
	display: inline-block;
	background: var(--steel);
}
.dot.on {
	background: var(--cobalt);
}
.dot.idle {
	background: var(--carbon);
	box-shadow: inset 0 0 0 1px var(--steel);
}
.dot.ok {
	background: var(--ok);
}
.dot.fail {
	background: var(--danger);
}

.colophon {
	display: flex;
	gap: 8px;
	margin-top: auto;
	padding: 16px;
	font-family: var(--font-mono);
	font-size: 11px;
	letter-spacing: 0.02em;
	color: var(--muted);
	border-top: 1px solid var(--line);
}

.skeleton {
	opacity: 0.7;
	animation: pulse 1.2s ease-in-out infinite;
}
@keyframes pulse {
	50% {
		opacity: 0.35;
	}
}
.center-text {
	text-align: center;
}

.banner {
	margin: 12px 16px 0;
	padding: 10px 12px;
	border-left: 2px solid var(--danger);
	border-radius: var(--r-sm);
	background: var(--danger-soft);
	color: var(--danger);
	font-size: 13px;
}

/* ---- navigation ----------------------------------------------------------- */
.tabs {
	display: flex;
	gap: 4px;
	padding: 8px 16px 0;
	border-bottom: 1px solid var(--line);
}
.tabs button {
	border: none;
	background: none;
	padding: 8px 10px 10px;
	font: inherit;
	font-size: 14px;
	font-weight: 500;
	color: var(--muted);
	border-bottom: 2px solid transparent;
	margin-bottom: -1px;
	cursor: pointer;
	transition: color 150ms var(--ease);
}
.tabs button:hover {
	color: var(--text);
}
.settings-bar {
	align-items: center;
	gap: 8px;
	padding-bottom: 8px;
}
.tabs button.on {
	color: var(--text);
	border-bottom-color: var(--accent);
}

main {
	padding: 16px;
}

/* ---- shared building blocks used by the tab components ------------------- */
.stack {
	display: flex;
	flex-direction: column;
	gap: 12px;
}
.stack.tight,
.row.tight {
	gap: 4px;
}
.row {
	display: flex;
	align-items: center;
	gap: 8px;
}
.row.wrap {
	flex-wrap: wrap;
	gap: 8px;
}
.row.end {
	justify-content: flex-end;
}
.grow {
	flex: 1;
}
.min0 {
	min-width: 0;
}
.truncate {
	overflow: hidden;
	text-overflow: ellipsis;
	white-space: nowrap;
}
.strong {
	font-weight: 500;
}
.card {
	background: var(--surface);
	border: 1px solid var(--line);
	border-radius: var(--r-lg);
	padding: 10px 12px;
	transition: border-color 150ms var(--ease);
}
.card.active {
	border-color: var(--accent);
}
.clickable {
	cursor: pointer;
}
.meta {
	font-size: 12px;
	color: var(--muted);
	display: flex;
	gap: 6px;
	flex-wrap: wrap;
}
.meta.truncate {
	display: block;
}
.muted,
.hint {
	color: var(--muted);
	font-size: 13px;
	margin: 0;
}
.section-title {
	margin: 0;
	font-size: 16px;
	font-weight: 500;
	letter-spacing: -0.01em;
}
/* Badges are labels: mono, small, no pills. */
.badge {
	font-family: var(--font-mono);
	font-size: 10px;
	letter-spacing: 0.04em;
	text-transform: uppercase;
	padding: 1px 5px;
	border-radius: var(--r-sm);
	background: var(--brand-pale);
	color: var(--brand);
	white-space: nowrap;
}
.badge.subtle {
	background: transparent;
	color: var(--muted);
	box-shadow: inset 0 0 0 1px var(--line);
}
.badge.warn {
	background: transparent;
	color: var(--warn);
	box-shadow: inset 0 0 0 1px var(--warn);
}
.status.normal {
	color: var(--ok);
}
.status.offline,
.status.error {
	color: var(--danger);
}
.warn-text {
	color: var(--warn);
}
.error {
	color: var(--danger);
	margin: 0;
	font-size: 13px;
}
.small-text {
	font-size: 12px;
}
.mono,
code {
	font-family: var(--font-mono);
	font-size: 12px;
}
pre.settings {
	margin: 4px 0 0;
	font-family: var(--font-mono);
	font-size: 11px;
	background: var(--bg);
	border: 1px solid var(--line);
	padding: 8px;
	border-radius: var(--r-md);
	overflow-x: auto;
}

fieldset {
	/* browsers default fieldsets to min-width: min-content, letting long text overflow */
	min-width: 0;
	border: 1px solid var(--line);
	border-radius: var(--r-lg);
	padding: 8px 12px 12px;
	margin: 0;
	display: flex;
	flex-direction: column;
	gap: 8px;
}
legend {
	font-size: 13px;
	font-weight: 500;
	padding: 0 4px;
}
.field {
	display: flex;
	flex-direction: column;
	gap: 4px;
	font-size: 12px;
	color: var(--muted);
	min-width: 0;
}
.grid2 {
	display: grid;
	grid-template-columns: 1fr 1fr;
	gap: 8px;
}
.grid4 {
	display: grid;
	grid-template-columns: repeat(4, 1fr);
	gap: 8px;
}
.check {
	font-size: 13px;
	cursor: pointer;
}
input[type="radio"],
input[type="checkbox"] {
	accent-color: var(--brand-fill);
}

input:not([type="radio"]):not([type="checkbox"]),
select,
textarea {
	font: inherit;
	font-size: 14px;
	color: var(--text);
	background: var(--surface);
	border: 1px solid var(--line);
	border-radius: var(--r-md);
	padding: 6px 8px;
	min-height: var(--control-height);
	min-width: 0;
	width: 100%;
	transition: border-color 150ms var(--ease);
}
input:focus,
select:focus,
textarea:focus {
	outline: none;
	border-color: var(--accent);
	box-shadow: 0 0 0 1px var(--accent);
}

button.primary,
button.ghost {
	font: inherit;
	font-size: 14px;
	font-weight: 500;
	border-radius: var(--r-md);
	padding: 6px 14px;
	min-height: var(--control-height);
	cursor: pointer;
	display: inline-flex;
	align-items: center;
	gap: 6px;
	transition: background 150ms var(--ease), border-color 150ms var(--ease);
}
button.primary {
	background: var(--brand-fill);
	color: #fff;
	border: 1px solid var(--brand-fill);
}
button.primary:hover:not(:disabled) {
	background: var(--cobalt);
	border-color: var(--cobalt);
}
button.ghost {
	background: var(--surface);
	color: var(--text);
	border: 1px solid var(--line);
}
button.ghost:hover:not(:disabled) {
	border-color: var(--muted);
}
button.small {
	padding: 6px 10px;
	min-height: var(--control-height);
	font-size: 13px;
}
button.icon {
	border: none;
	background: none;
	color: var(--muted);
	padding: 6px;
	min-width: 32px;
	min-height: 32px;
	border-radius: var(--r-md);
	cursor: pointer;
	display: grid;
	place-items: center;
	flex: none;
	transition: background 150ms var(--ease), color 150ms var(--ease);
}
button.icon:hover:not(:disabled) {
	background: var(--brand-pale);
	color: var(--brand);
}
button.icon.danger:hover:not(:disabled) {
	background: var(--danger-soft);
	color: var(--danger);
}
button:disabled {
	opacity: 0.45;
	cursor: default;
}

.toast {
	position: fixed;
	z-index: 20;
	left: 50%;
	bottom: 16px;
	transform: translateX(-50%);
	max-width: calc(100% - 32px);
	background: var(--obsidian);
	color: var(--ivory);
	border: 1px solid var(--border-dark);
	padding: 10px 14px;
	border-radius: var(--r-md);
	font-size: 13px;
	cursor: pointer;
}
.toast.error {
	border-left: 2px solid var(--danger);
}
</style>
