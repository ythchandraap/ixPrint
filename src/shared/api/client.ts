export { invoke } from "@tauri-apps/api/core";

/** False when the page is opened in a normal browser instead of the ix.print app. */
export const isInApp = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Tauri rejects with the Rust error string; normalise anything else. */
export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
