import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { execFileSync } from "node:child_process";

const [target, bundleRoot] = process.argv.slice(2);
if (!target || !bundleRoot) throw new Error("Usage: bun scripts/write-artifact-manifest.mjs <target> <bundle-root>");

const packageVersion = JSON.parse(readFileSync("package.json", "utf8")).version;
const tauriVersion = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
if (packageVersion !== tauriVersion) {
  throw new Error(`Frontend package version ${packageVersion} does not match Tauri version ${tauriVersion}`);
}
const version = tauriVersion;
const commit = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim();
const artifactFiles = [];

function visit(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) visit(path);
    else if (/\.(dmg|msi|exe|appimage|deb)$/i.test(entry.name)) artifactFiles.push(path);
  }
}

visit(bundleRoot);
if (artifactFiles.length === 0) throw new Error(`No installer files found under ${bundleRoot}`);

const rows = artifactFiles.sort().map((path) => {
  const sha256 = createHash("sha256").update(readFileSync(path)).digest("hex");
  return `${sha256}  ${relative(bundleRoot, path)}`;
});
const manifest = [
  `version=${version}`,
  `commit=${commit}`,
  `target=${target}`,
  "sha256:",
  ...rows,
  "",
].join("\n");
writeFileSync(join(bundleRoot, "artifact-manifest.txt"), manifest);
console.log(manifest);
