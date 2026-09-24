import { onBeforeUnmount, onMounted } from "vue";

export function usePolling(fn: () => void | Promise<void>, ms: number) {
	let timer: number | undefined;

	function stop() {
		if (timer !== undefined) window.clearInterval(timer);
		timer = undefined;
	}

	function start(runImmediately: boolean) {
		if (document.visibilityState !== "visible" || timer !== undefined) return;
		if (runImmediately) void fn();
		timer = window.setInterval(() => void fn(), ms);
	}

	function onVisibilityChange() {
		if (document.visibilityState === "visible") start(true);
		else stop();
	}

	onMounted(() => {
		document.addEventListener("visibilitychange", onVisibilityChange);
		start(false);
	});
	onBeforeUnmount(() => {
		document.removeEventListener("visibilitychange", onVisibilityChange);
		stop();
	});
}
