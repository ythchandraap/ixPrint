import { invoke } from "../client";
import type { IncomingJob, IncomingPrintResult, IncomingSource, IncomingTargetCheck } from "../types";

export const incomingCommands = {
  incomingDocuments: () => invoke<IncomingJob[]>("list_incoming_documents"),
  stageIncomingPdf: (sourcePath: string, source: IncomingSource, copies = 1, osJobId: string | null = null) =>
    invoke<IncomingJob>("stage_incoming_pdf", { sourcePath, source, copies, osJobId }),
  stageIncomingFile: (sourcePath: string, source: IncomingSource, copies = 1) =>
    invoke<IncomingJob>("stage_incoming_file", { sourcePath, source, copies }),
  previewIncomingPdfPage: (jobId: string, page: number) => invoke<string>("preview_incoming_pdf_page", { jobId, page }),
  checkIncomingTarget: (jobId: string, printer: string) =>
    invoke<IncomingTargetCheck>("check_incoming_target", { jobId, printer }),
  submitIncomingDocument: (jobId: string, printer: string) =>
    invoke<IncomingPrintResult>("submit_incoming_document", { jobId, printer }),
  cancelIncomingDocument: (jobId: string) => invoke<IncomingJob>("cancel_incoming_document", { jobId }),
};
