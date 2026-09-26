import { computed, reactive } from "vue";
import { describe, expect, it } from "vitest";
import type { Template } from "@/shared/api";
import { useDesignLayers } from "./useDesignLayers";

function setup(template = sample()) {
  const t = reactive(template);
  const list = computed(() => t.layout?.mode === "list" ? t.layout : null);
  const height = computed(() => list.value ? list.value.header_mm + list.value.row_mm + list.value.footer_mm : t.height_mm);
  const layers = useDesignLayers(t, list, height, () => {});
  return { t, ...layers };
}

function sample(): Template {
  return {
    width_mm: 60,
    height_mm: 40,
    elements: [
      { type: "rect", id: "e1", x: 2, y: 2, w: 10, h: 5, stroke: 1, fill: false },
      { type: "rect", id: "e2", x: 20, y: 2, w: 10, h: 5, stroke: 1, fill: false },
      { type: "rect", id: "e3", x: 2, y: 10, w: 10, h: 5, stroke: 1, fill: false },
    ],
    groups: [],
    sample: {},
  };
}

describe("design group operations", () => {
  it("groups selected siblings without changing their position or z-order, then nests a child group", () => {
    const state = setup();
    const before = JSON.stringify(state.t.elements);
    state.selectNode({ kind: "element", id: "e1" });
    state.selectNode({ kind: "element", id: "e2" }, true);
    expect(state.createGroup()).toBeNull();
    const outer = state.selected.value[0].id;
    expect(state.focusGroupId.value).toBeNull();
    expect(state.t.groups?.[0]).toMatchObject({ name: "Group 1", parentGroupId: null, elementIds: ["e1", "e2"] });
    expect(JSON.stringify(state.t.elements)).toBe(before);

    state.enterGroup(outer);
    state.selectNode({ kind: "element", id: "e1" });
    state.selectNode({ kind: "element", id: "e2" }, true);
    expect(state.createGroup()).toBeNull();
    const inner = state.selected.value[0].id;
    expect(state.t.groups?.find((group) => group.id === inner)?.parentGroupId).toBe(outer);
    expect(state.t.groups?.find((group) => group.id === outer)?.elementIds).toEqual([]);
    expect(state.t.elements.map((element) => element.id)).toEqual(["e1", "e2", "e3"]);
  });

  it("rejects cycles and moves groups without changing their visual coordinates", () => {
    const state = setup();
    state.selectNode({ kind: "element", id: "e1" });
    state.selectNode({ kind: "element", id: "e2" }, true);
    state.createGroup();
    const outer = state.selected.value[0].id;
    state.enterGroup(outer);
    state.selectNode({ kind: "element", id: "e1" });
    state.selectNode({ kind: "element", id: "e2" }, true);
    state.createGroup();
    const inner = state.selected.value[0].id;
    const before = JSON.stringify(state.t.elements);
    state.setFocus(null);
    state.selectNode({ kind: "group", id: outer });
    expect(state.moveSelectionTo(inner)).toContain("cycle");
    expect(JSON.stringify(state.t.elements)).toBe(before);
    expect(state.moveSelectionTo(null)).toBeNull();
    expect(state.focusGroupId.value).toBeNull();
    expect(state.t.groups?.find((group) => group.id === outer)?.parentGroupId).toBeNull();
  });

  it("moves a nested group into a different destination and focuses that destination", () => {
    const template = sample();
    template.groups = [
      { id: "destination", name: "Destination", parentGroupId: null, elementIds: ["e1"] },
      { id: "source", name: "Source", parentGroupId: null, elementIds: ["e2"] },
    ];
    const state = setup(template);
    const before = JSON.stringify(state.t.elements);
    state.selectNode({ kind: "group", id: "source" });
    expect(state.moveSelectionTo("destination")).toBeNull();
    expect(state.t.groups?.find((group) => group.id === "source")?.parentGroupId).toBe("destination");
    expect(state.focusGroupId.value).toBe("destination");
    expect(JSON.stringify(state.t.elements)).toBe(before);
  });

  it("ungroups direct members at the parent level and removes empty groups", () => {
    const state = setup();
    state.selectNode({ kind: "element", id: "e1" });
    state.selectNode({ kind: "element", id: "e2" }, true);
    state.createGroup();
    const groupId = state.selected.value[0].id;
    state.selectNode({ kind: "group", id: groupId });
    expect(state.ungroupSelection()).toBeNull();
    expect(state.t.groups).toEqual([]);
    expect(state.selected.value).toEqual([{ kind: "element", id: "e1" }, { kind: "element", id: "e2" }]);
    expect(state.t.elements.map((element) => element.id)).toEqual(["e1", "e2", "e3"]);
  });

  it("blocks mixed list bands when grouping, but allows moving them to Canvas", () => {
    const template = sample();
    template.layout = { mode: "list", header_mm: 5, row_mm: 10, footer_mm: 10, footer: "last_page", roll: false };
    const state = setup(template);
    state.selectNode({ kind: "element", id: "e1" }); // header
    state.selectNode({ kind: "element", id: "e3" }, true); // row
    expect(state.createGroup()).toContain("header, row");
    expect(state.moveSelectionTo(null)).toBeNull();
  });

  it("blocks moving a group into a different list band", () => {
    const template = sample();
    template.layout = { mode: "list", header_mm: 5, row_mm: 10, footer_mm: 10, footer: "last_page", roll: false };
    template.groups = [
      { id: "header", name: "Header group", parentGroupId: null, elementIds: ["e1"] },
      { id: "row", name: "Row group", parentGroupId: null, elementIds: ["e3"] },
    ];
    const state = setup(template);
    state.selectNode({ kind: "group", id: "header" });
    expect(state.moveSelectionTo("row")).toContain("header, row");
    expect(state.t.groups?.find((group) => group.id === "header")?.parentGroupId).toBeNull();
  });

  it("orders Structure groups by the earliest z-order of their descendant elements", () => {
    const template = sample();
    template.groups = [
      { id: "later", name: "Later", parentGroupId: null, elementIds: ["e2"] },
      { id: "earlier", name: "Earlier", parentGroupId: null, elementIds: ["e1"] },
    ];
    const state = setup(template);
    expect(state.structureRows.value.filter((row) => row.depth === 0).map((row) => row.node.id)).toEqual(["earlier", "later", "e3"]);
  });
});
