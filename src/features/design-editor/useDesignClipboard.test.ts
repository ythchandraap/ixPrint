import { reactive, ref } from "vue";
import { describe, expect, it } from "vitest";
import type { Template } from "@/shared/api";
import { descendantElementIds } from "./groupModel";
import { useDesignClipboard } from "./useDesignClipboard";
import type { DesignNode } from "./useDesignLayers";

describe("design clipboard", () => {
  it("pastes nested groups with fresh IDs, preserved hierarchy and source z-order, offset as one unit", () => {
    const template = reactive<Template>({
      width_mm: 60, height_mm: 40,
      elements: [
        { type: "rect", id: "e2", x: 20, y: 2, w: 5, h: 5, stroke: 1, fill: false },
        { type: "rect", id: "e3", x: 30, y: 2, w: 5, h: 5, stroke: 1, fill: false },
        { type: "rect", id: "e1", x: 2, y: 2, w: 5, h: 5, stroke: 1, fill: false },
      ],
      groups: [
        { id: "ga", name: "Group A", parentGroupId: null, elementIds: ["e1"] },
        { id: "gb", name: "Group B", parentGroupId: "ga", elementIds: ["e2"] },
      ],
      sample: {},
    });
    const selected = ref<DesignNode[]>([{ kind: "group", id: "ga" }]);
    const focus = ref<string | null>(null);
    const moveSelectionBy = (dx: number, dy: number) => {
      for (const node of selected.value) {
        const ids = node.kind === "group" ? descendantElementIds(template, node.id) : new Set([node.id]);
        for (const element of template.elements.filter((item) => ids.has(item.id))) {
          if (element.type === "line") { element.x1 += dx; element.x2 += dx; element.y1 += dy; element.y2 += dy; }
          else { element.x += dx; element.y += dy; }
        }
      }
    };
    const clipboard = useDesignClipboard(template, selected, focus, moveSelectionBy, () => {});
    clipboard.copySelected();
    clipboard.pasteCopied();

    const copiedRoot = template.groups?.find((group) => selected.value[0].id === group.id);
    const copiedChild = template.groups?.find((group) => group.parentGroupId === copiedRoot?.id);
    expect(copiedRoot).toBeDefined();
    expect(copiedChild).toBeDefined();
    expect(copiedRoot?.id).not.toBe("ga");
    expect(copiedChild?.id).not.toBe("gb");
    expect(copiedRoot?.parentGroupId).toBeNull();
    expect(copiedRoot?.elementIds).not.toContain("e1");
    expect(copiedChild?.elementIds).not.toContain("e2");
    const newElements = template.elements.slice(3);
    expect(newElements.map((element) => element.id)).toEqual([copiedChild?.elementIds[0], copiedRoot?.elementIds[0]]);
    expect(newElements.map((element) => element.type === "line" ? [element.x1, element.y1] : [element.x, element.y])).toEqual([[22, 4], [4, 4]]);
    expect(selected.value).toEqual([{ kind: "group", id: copiedRoot?.id }]);
  });

  it("pastes multiple objects into the active group in their source element order", () => {
    const template = reactive<Template>({
      width_mm: 60, height_mm: 40,
      elements: [
        { type: "rect", id: "e1", x: 1, y: 1, w: 2, h: 2, stroke: 1, fill: false },
        { type: "rect", id: "e2", x: 5, y: 1, w: 2, h: 2, stroke: 1, fill: false },
      ],
      groups: [{ id: "g", name: "Group", parentGroupId: null, elementIds: ["e1", "e2"] }], sample: {},
    });
    const selected = ref<DesignNode[]>([{ kind: "element", id: "e2" }, { kind: "element", id: "e1" }]);
    const focus = ref<string | null>("g");
    const clipboard = useDesignClipboard(template, selected, focus, (dx, dy) => {
      for (const node of selected.value) {
        const element = template.elements.find((item) => item.id === node.id);
        if (element && element.type !== "line") { element.x += dx; element.y += dy; }
      }
    }, () => {});
    clipboard.copySelected();
    clipboard.pasteCopied();
    expect(template.elements.slice(2).map((element) => element.type === "line" ? element.x1 : element.x)).toEqual([3, 7]);
    expect(template.groups?.[0].elementIds).toEqual(expect.arrayContaining(["e1", "e2", ...template.elements.slice(2).map((element) => element.id)]));
  });
});
