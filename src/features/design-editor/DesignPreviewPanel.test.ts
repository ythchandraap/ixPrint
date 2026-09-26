import { flushPromises, mount } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { api, type Template } from "@/shared/api";
import DesignPreviewPanel from "./DesignPreviewPanel.vue";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

const template: Template = {
  width_mm: 60,
  height_mm: 40,
  elements: [],
  sample: {},
};

describe("data and preview workspace", () => {
  it("keeps JSON input while switching sources and ignores an older preview response", async () => {
    const first = deferred<[string[], number]>();
    const latest = deferred<[string[], number]>();
    const samplePreview = vi.spyOn(api, "previewPrint").mockReturnValue(first.promise);
    const jsonPreview = vi.spyOn(api, "previewPrintData").mockReturnValue(latest.promise);
    const wrapper = mount(DesignPreviewPanel, {
      props: {
        template: structuredClone(template),
        list: null,
        fields: [],
        autoFields: [],
        autoHint: {},
        exampleBody: '{"preset":"receipt","data":[{"name":"Example"}]}',
      },
    });

    const firstRun = (wrapper.vm as unknown as { previewCurrent: () => Promise<void> }).previewCurrent();
    expect(samplePreview).toHaveBeenCalledOnce();

    await wrapper.get('[role="tablist"][aria-label="Data source"] button:nth-child(2)').trigger("click");
    const json = wrapper.get("textarea");
    await json.setValue('{"preset":"receipt","data":[{"name":"Latest"}]}');
    const latestRun = (wrapper.vm as unknown as { previewCurrent: () => Promise<void> }).previewCurrent();
    expect(jsonPreview).toHaveBeenCalledOnce();

    latest.resolve([["newest-page.png"], 2]);
    await latestRun;
    await flushPromises();
    first.resolve([["old-page.png"], 1]);
    await firstRun;
    await flushPromises();

    expect(wrapper.get(".preview-page-image img").attributes("src")).toBe("newest-page.png");
    expect(wrapper.get(".preview-page-controls").text()).toContain("Page 1 of 2");

    await wrapper.get('[role="tablist"][aria-label="Data source"] button:first-child').trigger("click");
    await wrapper.get('[role="tablist"][aria-label="Data source"] button:nth-child(2)').trigger("click");
    expect((wrapper.get("textarea").element as HTMLTextAreaElement).value).toBe('{"preset":"receipt","data":[{"name":"Latest"}]}');
    samplePreview.mockRestore();
    jsonPreview.mockRestore();
  });
});
