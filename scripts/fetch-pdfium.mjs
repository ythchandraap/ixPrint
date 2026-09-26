// Downloads the PDFium shared libraries bundled with ixPrint into src-tauri/pdfium/<target>/.
// Usage: bun run fetch-pdfium            (all supported targets)
//        bun run fetch-pdfium mac-arm64  (only the listed targets)
//
// The version must match the `pdfium_*` feature of the pdfium-render crate (see src-tauri/Cargo.toml).
import { mkdirSync, existsSync, rmSync, writeFileSync, copyFileSync, cpSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";

const PDFIUM_BUILD = "7881";
const TARGETS = {
  "win-x64": {
    library: "bin/pdfium.dll",
    sha256: "73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08",
  },
  "mac-x64": {
    library: "lib/libpdfium.dylib",
    sha256: "6dedf83990e0e3d6b7c93c9e7589c5a126b0ae14b7464d76120cff7a26afb18b",
  },
  "mac-arm64": {
    library: "lib/libpdfium.dylib",
    sha256: "52e94ca5aa8847934330daf3f8150c190682c5ca93831468794f8b90d4392e40",
  },
  "linux-x64": {
    library: "lib/libpdfium.so",
    sha256: "1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d",
  },
};

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "pdfium");
const requested = process.argv.slice(2);
const targets = requested.length ? requested : Object.keys(TARGETS);

for (const target of targets) {
  const targetInfo = TARGETS[target];
  if (!targetInfo) {
    console.error(`Unknown target "${target}". Known: ${Object.keys(TARGETS).join(", ")}`);
    process.exit(1);
  }
  const { library: libPath, sha256: expectedSha256 } = targetInfo;
  const outDir = join(root, target);
  const libName = libPath.split("/").pop();

  const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${PDFIUM_BUILD}/pdfium-${target}.tgz`;
  console.log(`↓ ${target}  ${url}`);
  const res = await fetch(url);
  if (!res.ok) {
    console.error(`Download failed (${res.status}) for ${url}`);
    process.exit(1);
  }

  const work = join(tmpdir(), `ixprint-pdfium-${target}-${Date.now()}`);
  mkdirSync(work, { recursive: true });
  const archive = join(work, "pdfium.tgz");
  const archiveBytes = Buffer.from(await res.arrayBuffer());
  const actualSha256 = createHash("sha256").update(archiveBytes).digest("hex");
  if (actualSha256 !== expectedSha256) {
    rmSync(work, { recursive: true, force: true });
    throw new Error(`SHA-256 mismatch for ${target}: expected ${expectedSha256}, got ${actualSha256}`);
  }
  writeFileSync(archive, archiveBytes);

  // `tar` ships with Windows 10+, macOS and Linux.
  const tar = spawnSync("tar", ["-xzf", archive, "-C", work], { stdio: "inherit" });
  if (tar.status !== 0) {
    rmSync(work, { recursive: true, force: true });
    console.error(`Failed to extract ${archive}`);
    process.exit(1);
  }

  if (!existsSync(join(work, "LICENSE")) || !existsSync(join(work, "licenses"))) {
    rmSync(work, { recursive: true, force: true });
    throw new Error(`PDFium ${PDFIUM_BUILD} archive for ${target} must contain LICENSE and licenses/`);
  }

  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });
  // The temporary directory and workspace can be on different volumes on
  // Windows GitHub runners (for example C:\Temp and D:\a). Rename then
  // fails with EXDEV, so copy the extracted files before removing the temp.
  copyFileSync(join(work, libPath), join(outDir, libName));
  copyFileSync(join(work, "LICENSE"), join(outDir, "LICENSE"));
  cpSync(join(work, "licenses"), join(outDir, "licenses"), { recursive: true });
  writeFileSync(join(outDir, "PDFIUM_BUILD"), `${PDFIUM_BUILD}\nsha256=${expectedSha256}\n`);
  rmSync(work, { recursive: true, force: true });
  console.log(`✓ ${target} → ${join(outDir, libName)}`);
}

console.log("Installed:", readdirSync(root).filter((d) => !d.startsWith(".") && d !== "lib").join(", "));
