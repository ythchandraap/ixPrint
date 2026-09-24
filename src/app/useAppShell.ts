import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { api, errorText, isInApp, type Config, type PrinterView } from "@/shared/api";

export function useAppShell() {
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

	return {
		PRODUCT, BRAND, inApp, version, serverState, serverError, bridgeEnabled, bridgeChanging,
		localIp, ipState, configError, os, engineError, printers, config, selectedPrinter, tab,
		printPageOpen, requestedPreset, tabLabel, settingsOpen, onboardingOpen, toast, refreshing,
		printersLoaded, notify, completeOnboarding, refreshPrinters, openDocs, osLabel, bridgeHost,
		address, copy, toggleDefaultPrinter, testPages, setTestPages, openPrintPage, toggleSettings,
		closePrintPage, selectTab, reload, setConfig, toggleBridge,
	};
}
