import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { TemplateElement } from "@/shared/api";
import DesignLayersPanel from "./DesignLayersPanel.vue";

describe("design layers panel", () => {
  it("keeps Structure free of reorder controls and shows group paths in Layer order", async () => {
    const element: TemplateElement = { type: "rect", id: "e1", x: 1, y: 1, w: 10, h: 10, stroke: 1, fill: false };
    const wrapper = mount(DesignLayersPanel, {
      props: {
        rows: [{ element, index: 0, label: "Rectangle", groupPath: "Group A / Group B" }],
        structureRows: [
          { node: { kind: "group", id: "ga" }, depth: 0, label: "Group A", group: { id: "ga", name: "Group A" }, hasChildren: true, sortIndex: 0 },
          { node: { kind: "group", id: "gb" }, depth: 1, label: "Group B", group: { id: "gb", name: "Group B" }, hasChildren: true, sortIndex: 0 },
          { node: { kind: "element", id: "e1" }, depth: 2, label: "Rectangle", hasChildren: false, sortIndex: 0 },
        ],
        selected: [],
        hasCopied: false,
        moveTargets: [{ id: null, label: "Canvas" }, { id: "ga", label: "Group A" }, { id: "gb", label: "Group A / Group B" }],
        canGroup: false,
        canUngroup: false,
      },
    });
    expect(wrapper.findAll(".structure-row")).toHaveLength(3);
    expect(wrapper.find(".layer-order-actions").exists()).toBe(false);
    const tabs = wrapper.findAll('[role="tab"]');
    await tabs[1].trigger("click");
    expect(wrapper.find(".layer-order-actions").exists()).toBe(true);
    expect(wrapper.text()).toContain("Group A / Group B");
  });
});
