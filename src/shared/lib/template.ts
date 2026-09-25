import type { Template } from "../api/types";

/** Fields ix.print fills in itself. */
export const AUTO_FIELDS = ["no", "halaman", "total_halaman", "jumlah_data"];
/** Automatic fields a design uses ({{no}}, {{halaman}}...); clients may send their own values. */
export function templateAutoFields(t: Template): string[] {
  const names = new Set<string>();
  for (const el of t.elements) {
    const src =
      el.type === "text"
        ? el.text
        : el.type === "barcode" || el.type === "qr"
          ? el.value
          : el.type === "image"
            ? el.src
            : "";
    for (const m of src.matchAll(/\{\{\s*([A-Za-z0-9_.-]+)\s*\}\}/g)) {
      if (AUTO_FIELDS.includes(m[1])) names.add(m[1]);
    }
  }
  return AUTO_FIELDS.filter((f) => names.has(f));
}

/** Placeholder names used by a template, sorted (same rule as the Rust side). */
export function templateFields(t: Template): string[] {
  const names = new Set<string>();
  for (const el of t.elements) {
    const src =
      el.type === "text"
        ? el.text
        : el.type === "barcode" || el.type === "qr"
          ? el.value
          : el.type === "image" && el.src.includes("{{")
            ? el.src
            : "";
    for (const m of src.matchAll(/\{\{\s*([A-Za-z0-9_.-]+)\s*\}\}/g)) {
      if (!AUTO_FIELDS.includes(m[1])) names.add(m[1]);
    }
  }
  return [...names].sort();
}
