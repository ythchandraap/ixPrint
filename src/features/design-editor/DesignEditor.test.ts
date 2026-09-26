import { flushPromises, mount } from "@vue/test-utils";
import { defineComponent, h, type Component } from "vue";
import { afterEach, describe, expect, it, vi } from "vitest";
import { api, type Template } from "@/shared/api";
import DesignPropertiesPanel from "./DesignPropertiesPanel.vue";
import DesignToolbar from "./DesignToolbar.vue";
import DesignEditor from "./DesignEditor.vue";

let viewportWidth = 1000;
class TestResizeObserver {
  constructor(private readonly callback: ResizeObserverCallback) {}
  observe() {
    this.callback([{ contentRect: { width: viewportWidth, height: 720 } } as ResizeObserverEntry], this as unknown as ResizeObserver);
  }
  unobserve() {}
  disconnect() {}
}

const ToolbarStub = defineComponent({
  name: "DesignToolbar",
  emits: ["add", "toggle-inspector"],
  setup(_, { emit }) {
    return () => h("div", [
      h("button", { class: "add-text", onClick: () => emit("add", "text") }, "Add text"),
      h("button", { class: "inspector-toggle", onClick: () => emit("toggle-inspector") }, "Inspector"),
    ]);
  },
});
const CanvasStub = defineComponent({
  name: "DesignCanvas",
  emits: ["select-from-canvas"],
  setup(_, { emit }) { return () => h("button", { class: "canvas-select", onClick: () => emit("select-from-canvas", "e1") }, "Select canvas object"); },
});
const LayersStub = defineComponent({
  name: "DesignLayersPanel",
  emits: ["select-node"],
  setup(_, { emit }) { return () => h("button", { class: "layers-select", onClick: () => emit("select-node", { kind: "group", id: "g1" }) }, "Select from layers"); },
});
const PropertiesStub = defineComponent({
  name: "DesignPropertiesPanel",
  props: ["groupSummary"],
  setup(props) { return () => h("div", { class: "property-summary" }, props.groupSummary ? `${props.groupSummary.name}:${props.groupSummary.count}` : "Object"); },
});
const PreviewStub = defineComponent({
  name: "DesignPreviewPanel",
  setup(_, { expose }) { expose({ previewCurrent: vi.fn() }); return () => h("div"); },
});

const sample: Template = {
  width_mm: 60,
  height_mm: 40,
  elements: [{ type: "rect", id: "e1", x: 2, y: 2, w: 10, h: 8, stroke: 0.5, fill: false }],
  groups: [{ id: "g1", name: "Group A", parentGroupId: null, elementIds: ["e1"] }],
  sample: {},
};

function setup(width = 1000, realPagePanel = false) {
  viewportWidth = width;
  vi.stubGlobal("ResizeObserver", TestResizeObserver);
  const saveDesign = vi.fn().mockResolvedValue(false);
  const stubs: Record<string, boolean | Component> = {
    DesignToolbar: ToolbarStub,
    DesignCanvas: CanvasStub,
    DesignLayersPanel: LayersStub,
    DesignPropertiesPanel: PropertiesStub,
    DesignPagePanel: true,
    DesignPreviewPanel: PreviewStub,
    DesignApiPanel: true,
  };
  if (realPagePanel) delete stubs.DesignPagePanel;
  const wrapper = mount(DesignEditor, {
    attachTo: document.body,
    props: {
      presetName: "receipt",
      template: structuredClone(sample),
      localIp: "127.0.0.1",
      serverPort: 8282,
      saveDesign,
      saving: false,
      saveError: "Storage is unavailable",
      leaveRequestToken: 0,
    },
    global: {
      stubs,
    },
  });
  return { wrapper, saveDesign };
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("design editor workspace state", () => {
  it("keeps the editor and dirty draft visible after preset persistence fails", async () => {
    vi.spyOn(api, "previewTemplate").mockResolvedValue("data:image/png;base64,preview");
    const { wrapper, saveDesign } = setup();
    await wrapper.get(".add-text").trigger("click");
    await wrapper.get(".editor-save").trigger("click");
    await flushPromises();

    expect(saveDesign).toHaveBeenCalledOnce();
    expect(wrapper.find(".editor-save").exists()).toBe(true);
    expect(wrapper.get(".save-state-indicator").attributes("aria-label")).toBe("Unsaved changes");
    expect(wrapper.get(".save-state-indicator").classes()).toContain("dirty");
    expect(wrapper.get('[role="alert"]').text()).toContain("Storage is unavailable");
    expect(wrapper.emitted("dirty-change")).toContainEqual([true]);
    wrapper.unmount();
  });

  it("shows a saved indicator beside the name after persistence succeeds", async () => {
    vi.spyOn(api, "previewTemplate").mockResolvedValue("data:image/png;base64,preview");
    const { wrapper, saveDesign } = setup();
    saveDesign.mockResolvedValue(true);
    await wrapper.setProps({ saveError: "" });
    await wrapper.get(".add-text").trigger("click");
    await wrapper.get(".editor-save").trigger("click");
    await flushPromises();

    expect(wrapper.get(".save-state-indicator").attributes("aria-label")).toBe("All changes saved");
    expect(wrapper.get(".save-state-indicator").classes()).not.toContain("dirty");
    expect(wrapper.get(".editor-preset-heading").element.contains(wrapper.get(".save-state-indicator").element)).toBe(true);
    wrapper.unmount();
  });

  it("routes canvas selection to Object but keeps Layers selected for tree selection", async () => {
    vi.spyOn(api, "previewTemplate").mockResolvedValue("data:image/png;base64,preview");
    const { wrapper } = setup();
    await wrapper.get("#inspector-tab-layers").trigger("click");
    await wrapper.get(".layers-select").trigger("click");
    await wrapper.get(".canvas-select").trigger("click");
    expect(wrapper.get("#inspector-tab-object").attributes("aria-selected")).toBe("true");
    expect(wrapper.findComponent(DesignPropertiesPanel).props("groupSummary")).toEqual({ name: "Group A", count: 1 });

    await wrapper.get("#inspector-tab-layers").trigger("click");
    await wrapper.get(".layers-select").trigger("click");
    expect(wrapper.get("#inspector-tab-layers").attributes("aria-selected")).toBe("true");
    expect(wrapper.findComponent(DesignToolbar).exists()).toBe(true);
    wrapper.unmount();
  });

  it("leaves the canvas full width on narrow workspaces and opens the inspector only on request", async () => {
    vi.spyOn(api, "previewTemplate").mockResolvedValue("data:image/png;base64,preview");
    const { wrapper } = setup(560);
    expect(wrapper.get(".design-inspector").isVisible()).toBe(false);
    await wrapper.get(".canvas-select").trigger("click");
    expect(wrapper.get(".design-inspector").isVisible()).toBe(false);
    await wrapper.get(".inspector-toggle").trigger("click");
    await flushPromises();
    expect(wrapper.get(".design-inspector").isVisible()).toBe(true);
    await wrapper.get("#inspector-tab-layers").trigger("click");
    const treeSelection = wrapper.get(".layers-select");
    await treeSelection.trigger("keydown", { key: "Escape" });
    expect(wrapper.get(".design-inspector").isVisible()).toBe(false);
    expect(wrapper.get(".inspector-toggle").element).toBe(document.activeElement);
    wrapper.unmount();
  });

  it("reveals and focuses an invalid page field that was hidden in another inspector tab", async () => {
    vi.spyOn(api, "previewTemplate").mockResolvedValue("data:image/png;base64,preview");
    const { wrapper, saveDesign } = setup(1000, true);
    const width = wrapper.get('#inspector-panel-page input[type="number"]');
    await width.setValue("3");
    await wrapper.get(".editor-save").trigger("click");
    await flushPromises();

    expect(wrapper.get("#inspector-tab-page").attributes("aria-selected")).toBe("true");
    expect(width.element).toBe(document.activeElement);
    expect(saveDesign).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});
