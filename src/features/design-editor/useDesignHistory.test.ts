import { afterEach, expect, it, vi } from "vitest";
import { defineComponent, h, nextTick, reactive } from "vue";
import { mount } from "@vue/test-utils";
import type { Template } from "@/shared/api";
import { useDesignHistory } from "./useDesignHistory";

afterEach(() => vi.useRealTimers());

it("undo and redo preserve the element array used by pointer interactions", async () => {
  vi.useFakeTimers();
  const template = reactive<Template>({
    width_mm: 60,
    height_mm: 40,
    sample: {},
    elements: [{ type: "rect", id: "box", x: 1, y: 1, w: 10, h: 10, stroke: 1, fill: false }],
  });
  const elements = template.elements;
  let history!: ReturnType<typeof useDesignHistory>;
  const wrapper = mount(defineComponent({
    setup() {
      history = useDesignHistory(template);
      return () => h("div");
    },
  }));

  const box = () => template.elements[0] as Extract<Template["elements"][number], { type: "rect" }>;
  box().x = 5;
  await nextTick();
  vi.advanceTimersByTime(300);
  expect(history.canUndo.value).toBe(true);

  history.undo();
  await nextTick();
  expect(box().x).toBe(1);
  expect(template.elements).toBe(elements);
  history.redo();
  await nextTick();
  expect(box().x).toBe(5);
  expect(template.elements).toBe(elements);
  wrapper.unmount();
});
