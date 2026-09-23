import { createApp } from "vue";
import App from "./App.vue";
import { api, isInApp } from "./api";

function errorMessage(value: unknown): string {
	if (value instanceof Error) return value.message;
	if (typeof value === "string") return value;
	try {
		return JSON.stringify(value) ?? String(value);
	} catch {
		return String(value);
	}
}

const report = (source: string, message: string, details?: string) => {
	if (isInApp()) void api.recordError(source, message, details).catch(() => {});
};

window.addEventListener("error", (event) => {
	report("javascript", event.message, event.error instanceof Error ? event.error.stack : `${event.filename}:${event.lineno}:${event.colno}`);
});
window.addEventListener("unhandledrejection", (event) => {
	const reason = event.reason;
	report("promise", errorMessage(reason), reason instanceof Error ? reason.stack : undefined);
});

if (import.meta.env.PROD) {
	// No webview menu (reload, view source, …) in release builds, except in text fields for copy/paste.
	document.addEventListener("contextmenu", (e) => {
		const t = e.target as HTMLElement | null;
		if (!t?.closest("input, textarea, [contenteditable=true]")) e.preventDefault();
	});
}

const app = createApp(App);
app.config.errorHandler = (error, _instance, info) => {
	console.error(error, info);
	report("vue", errorMessage(error), `${info}\n${error instanceof Error ? error.stack ?? "" : ""}`);
};
app.mount("#app");
