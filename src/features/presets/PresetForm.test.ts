import { flushPromises, mount } from "@vue/test-utils";
import { h } from "vue";
import { describe, expect, it, vi } from "vitest";
import { api, type Template } from "@/shared/api";
import PresetForm from "./PresetForm.vue";

describe("preset design save flow", () => {
  it("keeps the design editor open and exposes the save error when persistence fails", async () => {
    const savePreset = vi.spyOn(api, "savePreset").mockRejectedValue(new Error("Storage is unavailable"));
    const wrapper = mount(PresetForm, {
      props: { settings: {}, printers: [], localIp: "127.0.0.1", serverPort: 8282 },
      slots: {
        "design-editor": (slot: { template: Template; saveDesign: (template: Template) => Promise<boolean>; saveError: string }) => h("section", [
          h("button", { class: "save-design", onClick: () => void slot.saveDesign(slot.template) }, "Save design"),
          h("p", { role: "alert" }, slot.saveError),
        ]),
      },
    });

    await wrapper.get('input[placeholder="e.g. receipt"]').setValue("receipt");
    await wrapper.findAll("button").find((button) => button.text() === "Product label")!.trigger("click");
    await wrapper.get(".save-design").trigger("click");
    await flushPromises();

    expect(savePreset).toHaveBeenCalledOnce();
    expect(wrapper.find(".save-design").exists()).toBe(true);
    expect(wrapper.get('[role="alert"]').text()).toContain("Storage is unavailable");
    expect(wrapper.emitted("saved")).toBeUndefined();
    expect(wrapper.emitted("design-editor-active")).toContainEqual([true]);

    savePreset.mockRestore();
  });
});
