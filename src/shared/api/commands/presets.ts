import { invoke } from "../client";
import type { Config, Preset, Template, TemplateRecord, ImportPreview, ImportOptions } from "../types";

export const presetCommands = {
  savePreset: (name: string, preset: Preset, originalName?: string) =>
    invoke<Config>("save_preset", { name, preset, originalName: originalName ?? null }),
  deletePreset: (name: string) => invoke<Config>("delete_preset", { name }),
  /** PNG data URL rendered by PDFium, exactly as it prints. */
  /** PNG data URL rendered by PDFium, exactly as it prints. */
  previewTemplate: (template: Template, data: TemplateRecord, pxPerMm: number) =>
    invoke<string>("preview_template", { template, data, pxPerMm }),
  /** Real pagination with the sample data repeated `rows` times: up to 6 page images + total pages. */
  /** Real pagination with the sample data repeated `rows` times: up to 6 page images + total pages. */
  previewPrint: (template: Template, rows: number, pxPerMm: number) =>
    invoke<[string[], number]>("preview_print", { template, rows, pxPerMm }),
  exportPresets: (path: string, names: string[] | null, includeDefaults: boolean) =>
    invoke<number>("export_presets", { path, names, includeDefaults }),
  previewImport: (path: string) => invoke<ImportPreview>("preview_import", { path }),
  applyImport: (path: string, options: ImportOptions) =>
    invoke<{ config: Config; imported: string[] }>("apply_import", { path, options }),
};
