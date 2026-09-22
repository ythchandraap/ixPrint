// Downloads the PDFium shared libraries bundled with ixPrint into src-tauri/pdfium/<target>/.
// Usage: bun run fetch-pdfium            (all supported targets)
//        bun run fetch-pdfium mac-arm64  (only the listed targets)
//
// The version must match the `pdfium_*` feature of the pdfium-render crate (see src-tauri/Cargo.toml).
import { mkdirSync, existsSync, rmSync, writeFileSync, copyFileSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";

const PDFIUM_BUILD = "7881";
const TARGETS = {
  "win-x64": "bin/pdfium.dll",
  "win-arm64": "bin/pdfium.dll",
  "mac-x64": "lib/libpdfium.dylib",
  "mac-arm64": "lib/libpdfium.dylib",
  "linux-x64": "lib/libpdfium.so",
  "linux-arm64": "lib/libpdfium.so",
};

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "pdfium");
const requested = process.argv.slice(2);
const targets = requested.length ? requested : Object.keys(TARGETS);

for (const target of targets) {
  const libPath = TARGETS[target];
  if (!libPath) {
    console.error(`Unknown target "${target}". Known: ${Object.keys(TARGETS).join(", ")}`);
    process.exit(1);
  }
  const outDir = join(root, target);
  const libName = libPath.split("/").pop();
  if (existsSync(join(outDir, libName))) {
    console.log(`✓ ${target} already present`);
    continue;
  }

  const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/${PDFIUM_BUILD}/pdfium-${target}.tgz`;
  console.log(`↓ ${target}  ${url}`);
  const res = await fetch(url);
  if (!res.ok) {
    console.error(`Download failed (${res.status}) for ${url}`);
    process.exit(1);
  }

  const work = join(tmpdir(), `ixprint-pdfium-${target}-${Date.now()}`);
  mkdirSync(work, { recursive: true });
  const archive = join(work, "pdfium.tgz");
  writeFileSync(archive, Buffer.from(await res.arrayBuffer()));

  // `tar` ships with Windows 10+, macOS and Linux.
  const tar = spawnSync("tar", ["-xzf", archive, "-C", work], { stdio: "inherit" });
  if (tar.status !== 0) {
    console.error(`Failed to extract ${archive}`);
    process.exit(1);
  }

  mkdirSync(outDir, { recursive: true });
  // The temporary directory and workspace can be on different volumes on
  // Windows GitHub runners (for example C:\Temp and D:\a). Rename then
  // fails with EXDEV, so copy the extracted files before removing the temp.
  copyFileSync(join(work, libPath), join(outDir, libName));
  if (existsSync(join(work, "LICENSE"))) copyFileSync(join(work, "LICENSE"), join(outDir, "LICENSE"));
  rmSync(work, { recursive: true, force: true });
  console.log(`✓ ${target} → ${join(outDir, libName)}`);
}

console.log("Installed:", readdirSync(root).filter((d) => !d.startsWith(".") && d !== "lib").join(", "));
