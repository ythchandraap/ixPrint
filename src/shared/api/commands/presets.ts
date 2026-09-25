import { invoke } from "../client";
import type {
  Config,
  Preset,
  Template,
  TemplateRecord,
  ImportPreview,
  ImportOptions,
  SharedPresetListing,
} from "../types";

export const presetCommands = {
  savePreset: (name: string, preset: Preset, originalName?: string) =>
    invoke<Config>("save_preset", { name, preset, originalName: originalName ?? null }),
  savePresetDraft: (name: string, preset: Preset) => invoke<Config>("save_preset_draft", { name, preset }),
  publishPresetDraft: (name: string) => invoke<Config>("publish_preset_draft", { name }),
  discardPresetDraft: (name: string) => invoke<Config>("discard_preset_draft", { name }),
  restorePresetPrevious: (name: string) => invoke<Config>("restore_preset_previous", { name }),
  testPresetDraft: (name: string) => invoke<{ job_id: string }>("test_preset_draft", { name }),
  deletePreset: (name: string) => invoke<Config>("delete_preset", { name }),
  setPresetShared: (name: string, shared: boolean) => invoke<Config>("set_preset_shared", { name, shared }),
  setPresetListEndpointEnabled: (enabled: boolean) => invoke<Config>("set_preset_list_endpoint_enabled", { enabled }),
  /** PNG data URL rendered by PDFium, exactly as it prints. */
  /** PNG data URL rendered by PDFium, exactly as it prints. */
  previewTemplate: (template: Template, data: TemplateRecord, pxPerMm: number) =>
    invoke<string>("preview_template", { template, data, pxPerMm }),
  /** Real pagination with the sample data repeated `rows` times: up to 6 page images + total pages. */
  /** Real pagination with the sample data repeated `rows` times: up to 6 page images + total pages. */
  previewPrint: (template: Template, rows: number, pxPerMm: number) =>
    invoke<[string[], number]>("preview_print", { template, rows, pxPerMm }),
  previewPrintData: (template: Template, records: TemplateRecord[], header: TemplateRecord, pxPerMm: number) =>
    invoke<[string[], number]>("preview_print_data", { template, records, header, pxPerMm }),
  exportPresets: (path: string, names: string[] | null, includeDefaults: boolean) =>
    invoke<number>("export_presets", { path, names, includeDefaults }),
  previewImport: (path: string) => invoke<ImportPreview>("preview_import", { path }),
  applyImport: (path: string, options: ImportOptions) =>
    invoke<{ config: Config; imported: string[] }>("apply_import", { path, options }),
  getRemoteSharedPresets: (remoteId: string) => invoke<SharedPresetListing>("get_remote_shared_presets", { remoteId }),
  previewRemotePresetImport: (remoteId: string, name: string) =>
    invoke<ImportPreview>("preview_remote_preset_import", { remoteId, name }),
  applyRemotePresetImport: (remoteId: string, name: string, expectedSha256: string, options: ImportOptions) =>
    invoke<{ config: Config; imported: string[] }>("apply_remote_preset_import", {
      remoteId,
      name,
      expectedSha256,
      options,
    }),
};
