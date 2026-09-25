import { computed, defineComponent, h, reactive, ref } from "vue";
import { mount } from "@vue/test-utils";
import { expect, it } from "vitest";
import type { TemplateElement } from "@/shared/api";
import { useDesignPointer, type ResizeHandle } from "./useDesignPointer";

function setup() {
  const box = { type: "rect" as const, id: "box", x: 10, y: 20, w: 30, h: 40, stroke: 1, fill: false };
  const elements = reactive<TemplateElement[]>([{ ...box }]);
  let pointer!: ReturnType<typeof useDesignPointer>;
  const wrapper = mount(defineComponent({
    setup() {
      pointer = useDesignPointer(elements, ref(0), computed(() => 2), ref(false));
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
