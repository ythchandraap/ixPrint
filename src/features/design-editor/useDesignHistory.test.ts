import { afterEach, expect, it, vi } from "vitest";
import { defineComponent, h, nextTick, reactive } from "vue";
import { mount } from "@vue/test-utils";
import type { Template } from "@/shared/api";
import { useDesignHistory } from "./useDesignHistory";
import type { DesignNode } from "./useDesignLayers";

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

it("restores group metadata and the valid selection and focus for each snapshot", async () => {
  const template = reactive<Template>({
    width_mm: 60, height_mm: 40, sample: {}, groups: [],
    elements: [{ type: "rect", id: "box", x: 1, y: 1, w: 10, h: 10, stroke: 1, fill: false }],
  });
  let history!: ReturnType<typeof useDesignHistory>;
  let view: { selected: DesignNode[]; focusGroupId: string | null } = { selected: [{ kind: "element", id: "box" }], focusGroupId: null };
  const wrapper = mount(defineComponent({
    setup() {
      history = useDesignHistory(template);
      history.bindView(() => view, (next) => { view = next as typeof view; });
      return () => h("div");
    },
  }));
  template.groups!.push({ id: "group", name: "Group 1", parentGroupId: null, elementIds: ["box"] });
  history.commit();
  view = { selected: [{ kind: "group", id: "group" }], focusGroupId: null };
  history.markView();
  history.undo();
  await nextTick();
  expect(template.groups).toEqual([]);
  expect(view).toEqual({ selected: [{ kind: "element", id: "box" }], focusGroupId: null });
  history.redo();
  await nextTick();
  expect(template.groups).toHaveLength(1);
  expect(view).toEqual({ selected: [{ kind: "group", id: "group" }], focusGroupId: null });
  wrapper.unmount();
});
