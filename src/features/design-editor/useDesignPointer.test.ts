import { computed, defineComponent, h, reactive, ref } from "vue";
import { mount } from "@vue/test-utils";
import { expect, it } from "vitest";
import type { Template, TemplateElement } from "@/shared/api";
import { useDesignPointer, type ResizeHandle } from "./useDesignPointer";
import type { DesignNode } from "./useDesignLayers";

function setup() {
  const box = { type: "rect" as const, id: "box", x: 10, y: 20, w: 30, h: 40, stroke: 1, fill: false };
  const template = reactive<Template>({ width_mm: 100, height_mm: 100, elements: [{ ...box }], groups: [], sample: {} });
  const elements = template.elements;
  const selected = ref<DesignNode[]>([{ kind: "element", id: "box" }]);
  let pointer!: ReturnType<typeof useDesignPointer>;
  const wrapper = mount(defineComponent({
    setup() {
      pointer = useDesignPointer(template, selected, computed(() => 0), computed(() => 2), ref(false),
        (node) => new Set([node.id]), (id) => ({ kind: "element", id }), (node) => { selected.value = [node]; },
        (_ids, dx, dy) => [dx, dy], { beginTransaction() {}, endTransaction() {} });
      return () => h("div");
    },
  }));
  function drag(handle: ResizeHandle, dx: number, dy: number) {
    pointer.startDrag({
      clientX: 100,
      clientY: 100,
      currentTarget: document.createElement("span"),
      preventDefault() {},
      stopPropagation() {},
    } as unknown as PointerEvent, 0, handle);
    window.dispatchEvent(new MouseEvent("pointermove", { clientX: 100 + dx * 2, clientY: 100 + dy * 2 }));
    window.dispatchEvent(new Event("pointerup"));
  }
  return { box, elements, drag, wrapper };
}

it("resizes a rectangle from every side and corner while keeping opposite edges fixed", () => {
  const { box, elements, drag, wrapper } = setup();
  const cases: [ResizeHandle, number, number, number, number][] = [
    ["nw", 15, 26, 25, 34],
    ["n", 10, 26, 30, 34],
    ["ne", 10, 26, 35, 34],
    ["e", 10, 20, 35, 40],
    ["se", 10, 20, 35, 46],
    ["s", 10, 20, 30, 46],
    ["sw", 15, 20, 25, 46],
    ["w", 15, 20, 25, 40],
  ];
  for (const [handle, x, y, w, h] of cases) {
    Object.assign(elements[0], box);
    drag(handle, 5, 6);
    expect(elements[0]).toMatchObject({ x, y, w, h });
  }
  wrapper.unmount();
});

it("keeps a minimum size when a dragged edge crosses the opposite edge", () => {
  const { elements, drag, wrapper } = setup();
  drag("nw", 100, 100);
  expect(elements[0]).toMatchObject({ x: 39, y: 59, w: 1, h: 1 });
  wrapper.unmount();
});

it("moves all group descendants by one clamped delta in a single gesture", () => {
  const template = reactive<Template>({
    width_mm: 80,
    height_mm: 50,
    elements: [
      { type: "rect", id: "one", x: 10, y: 10, w: 20, h: 10, stroke: 1, fill: false },
      { type: "rect", id: "two", x: 50, y: 10, w: 20, h: 10, stroke: 1, fill: false },
    ],
    groups: [{ id: "group", name: "Group 1", parentGroupId: null, elementIds: ["one", "two"] }],
    sample: {},
  });
  const selected = ref<DesignNode[]>([{ kind: "group", id: "group" }]);
  let begins = 0;
  let ends = 0;
  let pointer!: ReturnType<typeof useDesignPointer>;
  const wrapper = mount(defineComponent({
    setup() {
      pointer = useDesignPointer(template, selected, computed(() => -1), computed(() => 2), ref(false),
        (node) => new Set(node.kind === "group" ? ["one", "two"] : [node.id]),
        () => ({ kind: "group", id: "group" }), (node) => { selected.value = [node]; },
        (ids, dx, dy) => {
          const boxes = template.elements.filter((element): element is Extract<TemplateElement, { type: "rect" }> => element.type === "rect" && ids.includes(element.id));
          const maxDx = Math.min(...boxes.map((element) => template.width_mm - element.x - element.w));
          return [Math.min(dx, maxDx), dy];
        }, { beginTransaction() { begins++; }, endTransaction() { ends++; } });
      return () => h("div");
    },
  }));
  pointer.startDrag({ clientX: 100, clientY: 100, currentTarget: document.createElement("span"), preventDefault() {}, stopPropagation() {} } as unknown as PointerEvent, 0, "move");
  window.dispatchEvent(new MouseEvent("pointermove", { clientX: 180, clientY: 110 }));
  window.dispatchEvent(new Event("pointerup"));
  expect(template.elements.map((element) => element.type === "line" ? [element.x1, element.y1] : [element.x, element.y])).toEqual([[20, 15], [60, 15]]);
  expect(begins).toBe(1);
  expect(ends).toBe(1);
  wrapper.unmount();
});
