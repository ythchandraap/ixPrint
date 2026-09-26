import { afterEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import IncomingReview from "./IncomingReview.vue";
import type { IncomingJob } from "@/shared/api";

const api = vi.hoisted(() => ({
  incomingDocuments: vi.fn(),
  remotes: vi.fn(),
  previewIncomingPdfPage: vi.fn(),
  checkIncomingTarget: vi.fn(),
  submitIncomingDocument: vi.fn(),
  cancelIncomingDocument: vi.fn(),
  jobStatus: vi.fn(),
}));

vi.mock("@/shared/api", () => ({ api, errorText: (error: unknown) => String(error) }));

const pendingJob: IncomingJob = {
  id: "job-1",
  source: "file_picker",
  document_name: "sample.pdf",
  pages: [{ width_pt: 595.28, height_pt: 841.89 }],
  copies: 1,
  state: "awaiting_destination",
  received_at: "2026-09-26T00:00:00Z",
};

describe("IncomingReview", () => {
  afterEach(() => vi.clearAllMocks());

  it("requires a compatible printer before submitting, then shows queued status", async () => {
    api.incomingDocuments
      .mockResolvedValueOnce([pendingJob])
      .mockResolvedValueOnce([
        { ...pendingJob, state: "queued", printer: "Office", spooler_job: "spool-4", message: "Sent to queue" },
      ]);
    api.remotes.mockResolvedValue([]);
    api.previewIncomingPdfPage.mockResolvedValue("data:image/png;base64,preview");
    api.checkIncomingTarget.mockResolvedValue({ compatible: true, supported_media: ["A4"] });
    api.submitIncomingDocument.mockResolvedValue({ job_id: "job-1", printer: "Office", sheets: 1, copies: 1 });
    api.jobStatus.mockResolvedValue({ state: "queued", position: 1 });

    const wrapper = mount(IncomingReview, {
      props: {
        initialJobId: pendingJob.id,
        printers: [{ name: "Office", status: "Ready", shared: "", os_default: false, is_default: false }],
      },
    });
    await flushPromises();

    const send = wrapper.get("button.primary");
    expect(send.attributes("disabled")).toBeDefined();
    await wrapper.get("#incoming-printer").setValue("Office");
    await flushPromises();
    expect(api.checkIncomingTarget).toHaveBeenCalledWith("job-1", "Office");
    expect(wrapper.get("button.primary").attributes("disabled")).toBeUndefined();

    await wrapper.get("button.primary").trigger("click");
    await flushPromises();
    expect(api.submitIncomingDocument).toHaveBeenCalledOnce();
    expect(wrapper.text()).toContain("Dikirim ke antrean");
    expect(wrapper.text()).toContain("Menunggu · posisi 1");
    wrapper.unmount();
  });
});
