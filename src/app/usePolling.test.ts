import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";
import { mount, type VueWrapper } from "@vue/test-utils";
import { usePolling } from "./usePolling";

let wrapper: VueWrapper | undefined;
const visibilityDescriptor = Object.getOwnPropertyDescriptor(document, "visibilityState");

function setVisibility(state: DocumentVisibilityState) {
	Object.defineProperty(document, "visibilityState", { configurable: true, value: state });
	document.dispatchEvent(new Event("visibilitychange"));
}

afterEach(() => {
	wrapper?.unmount();
	wrapper = undefined;
	vi.useRealTimers();
	if (visibilityDescriptor) Object.defineProperty(document, "visibilityState", visibilityDescriptor);
	else Reflect.deleteProperty(document, "visibilityState");
});

describe("usePolling", () => {
	it("polls while visible, stops while hidden, and fires immediately when visible again", () => {
		vi.useFakeTimers();
		Object.defineProperty(document, "visibilityState", { configurable: true, value: "visible" });
		const fn = vi.fn();
		const Probe = defineComponent({
			setup() {
				usePolling(fn, 1000);
				return () => h("div");
			},
		});
		wrapper = mount(Probe);

		expect(fn).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1000);
		expect(fn).toHaveBeenCalledTimes(1);

		setVisibility("hidden");
		vi.advanceTimersByTime(3000);
		expect(fn).toHaveBeenCalledTimes(1);

		setVisibility("visible");
		expect(fn).toHaveBeenCalledTimes(2);
		vi.advanceTimersByTime(1000);
		expect(fn).toHaveBeenCalledTimes(3);

		wrapper.unmount();
		wrapper = undefined;
		vi.advanceTimersByTime(2000);
		expect(fn).toHaveBeenCalledTimes(3);
	});
});
