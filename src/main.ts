import { createApp } from "vue";
import App from "./App.vue";

if (import.meta.env.PROD) {
	// No webview menu (reload, view source, …) in release builds, except in text fields for copy/paste.
	document.addEventListener("contextmenu", (e) => {
		const t = e.target as HTMLElement | null;
		if (!t?.closest("input, textarea, [contenteditable=true]")) e.preventDefault();
	});
}

createApp(App).mount("#app");
