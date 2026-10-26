// Syncs display branding from branding.json into src-tauri/tauri.conf.json
// so renaming the app = edit branding.json + rebuild.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const brandingPath = resolve(root, "branding.json");
const confPath = resolve(root, "src-tauri", "tauri.conf.json");

const branding = JSON.parse(readFileSync(brandingPath, "utf8"));
const name = typeof branding.appName === "string" ? branding.appName.trim() : "";
if (!name) {
  console.error("sync-branding: branding.json must have a non-empty \"appName\".");
  process.exit(1);
}

const raw = readFileSync(confPath, "utf8");
const conf = JSON.parse(raw);
conf.productName = name;
for (const win of conf.app?.windows ?? []) {
  win.title = name;
}

const next = JSON.stringify(conf, null, 2) + "\n";
if (next !== raw) {
  writeFileSync(confPath, next);
  console.log(`sync-branding: tauri.conf.json updated -> "${name}"`);
} else {
  console.log(`sync-branding: already "${name}"`);
}
