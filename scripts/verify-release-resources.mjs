import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";

const [target, bundleRoot] = process.argv.slice(2);
if (!target || !bundleRoot) throw new Error("Usage: bun scripts/verify-release-resources.mjs <target> <bundle-root>");

const expectedHashes = {
  "win-x64": "73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08",
  "mac-x64": "6dedf83990e0e3d6b7c93c9e7589c5a126b0ae14b7464d76120cff7a26afb18b",
  "mac-arm64": "52e94ca5aa8847934330daf3f8150c190682c5ca93831468794f8b90d4392e40",
  "linux-x64": "1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d",
};
if (!expectedHashes[target]) throw new Error(`Unsupported release target: ${target}`);

const temp = mkdtempSync(join(tmpdir(), "ixprint-release-resources-"));
let mountedImage = false;
let mountedPath = "";
const checkedInstallers = [];

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", ...options });
  if (result.status !== 0) throw new Error(`${command} failed while inspecting a final installer`);
}

function allFiles(directory) {
  const files = [];
  function collect(current) {
    let entries;
    try {
      entries = readdirSync(current, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) collect(path);
      else if (entry.isFile()) files.push(path);
    }
  }
  collect(directory);
  return files;
}

function verifyExtracted(installer, root) {
  const files = allFiles(root);
  const normalized = files.map((path) => path.replaceAll("\\", "/").toLowerCase());
  const licenseIndex = normalized.findIndex((path) => /\/pdfium\/lib\/license$/.test(path));
  if (licenseIndex < 0) throw new Error(`PDFium LICENSE missing from installer ${installer}`);

  const pdfiumDir = files[licenseIndex].replace(/[/\\]LICENSE$/i, "");
  const expectedMarkerPath = `${pdfiumDir.replaceAll("\\", "/")}/pdfium_build`.toLowerCase();
  const marker = files.find((path) => path.replaceAll("\\", "/").toLowerCase() === expectedMarkerPath);
  if (!marker) throw new Error(`PDFIUM_BUILD missing from installer ${installer}`);

  const markerContents = readText(marker);
  const expectedMarker = `7881\nsha256=${expectedHashes[target]}\n`;
  if (markerContents !== expectedMarker) throw new Error(`PDFium build/checksum marker mismatch in ${installer}`);

  const licensesDir = join(pdfiumDir, "licenses");
  const normalizedLicensesDir = `${licensesDir.replaceAll("\\", "/").toLowerCase()}/`;
  if (!normalized.some((path) => path.startsWith(normalizedLicensesDir))) {
    throw new Error(`PDFium licenses/ directory is empty or missing from installer ${installer}`);
  }
  if (!files.some((path) => path.toLowerCase().endsWith("third-party-notices.md"))) {
    throw new Error(`Application third-party notices are missing from installer ${installer}`);
  }
  for (const fontNotice of ["ibm-plex-ofl.txt", "instrument-sans-ofl.txt"]) {
    if (!normalized.some((path) => path.endsWith(`/licenses/${fontNotice}`))) {
      throw new Error(`Font notice ${fontNotice} is missing from installer ${installer}`);
    }
  }
  console.log(`Verified ${installer}: PDFium build 7881, checksum marker and ${files.length} extracted files`);
  checkedInstallers.push(installer);
}

function readText(path) {
  return readFileSync(path, "utf8");
}

try {
  const installers = allFiles(bundleRoot).filter((path) => {
    const lower = path.toLowerCase();
    if (target.startsWith("mac-")) return lower.endsWith(".dmg");
    if (target === "win-x64") return lower.endsWith(".msi") || lower.endsWith(".exe");
    return lower.endsWith(".deb") || lower.endsWith(".appimage");
  });
  if (installers.length === 0) throw new Error(`No final installer files found for ${target}`);

  for (const [index, installer] of installers.entries()) {
    const extraction = join(temp, `installer-${index}`);
    mkdirSync(extraction, { recursive: true });
    const lower = installer.toLowerCase();

    if (target.startsWith("mac-")) {
      const mount = join(extraction, "mount");
      mkdirSync(mount, { recursive: true });
      run("hdiutil", ["attach", "-nobrowse", "-readonly", "-mountpoint", mount, installer]);
      mountedImage = true;
      mountedPath = mount;
      verifyExtracted(installer, mount);
      run("hdiutil", ["detach", mount]);
      mountedImage = false;
      mountedPath = "";
    } else if (target === "win-x64" && lower.endsWith(".msi")) {
      const destination = join(extraction, "msi");
      mkdirSync(destination, { recursive: true });
      run("msiexec", ["/a", installer, "/qn", `TARGETDIR=${destination}`]);
      verifyExtracted(installer, destination);
    } else if (target === "win-x64" && lower.endsWith(".exe")) {
      const destination = join(extraction, "nsis");
      mkdirSync(destination, { recursive: true });
      const sevenZip = "C:\\Program Files\\7-Zip\\7z.exe";
      run(sevenZip, ["x", installer, `-o${destination}`, "-y"]);
      verifyExtracted(installer, destination);
    } else if (lower.endsWith(".deb")) {
      const destination = join(extraction, "deb");
      mkdirSync(destination, { recursive: true });
      run("dpkg-deb", ["-x", installer, destination]);
      verifyExtracted(installer, destination);
    } else {
      const destination = join(extraction, "appimage");
      mkdirSync(destination, { recursive: true });
      run(installer, ["--appimage-extract"], { cwd: destination });
      verifyExtracted(installer, join(destination, "squashfs-root"));
    }
  }

  if (checkedInstallers.length !== installers.length) throw new Error("Not every installer was inspected");
} finally {
  if (mountedImage) {
    spawnSync("hdiutil", ["detach", mountedPath], { stdio: "ignore" });
  }
  rmSync(temp, { recursive: true, force: true });
}
