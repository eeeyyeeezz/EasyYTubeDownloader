// Builds browser-specific copies of the extension into dist/chrome and dist/firefox.
// Chrome MV3 needs a service worker; Firefox MV3 needs background scripts and a gecko id.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const base = JSON.parse(fs.readFileSync(path.join(root, "manifest.base.json"), "utf8"));

const targets = {
  chrome: {
    ...base,
    background: { service_worker: "background.js" },
    minimum_chrome_version: "110",
  },
  firefox: {
    ...base,
    background: { scripts: ["background.js"] },
    browser_specific_settings: {
      gecko: {
        id: "easyytd@eeeyyeeezz.github.io",
        strict_min_version: "128.0",
        data_collection_permissions: { required: ["none"] },
      },
    },
  },
};

for (const [name, manifest] of Object.entries(targets)) {
  const out = path.join(root, "dist", name);
  fs.rmSync(out, { recursive: true, force: true });
  fs.mkdirSync(out, { recursive: true });
  fs.cpSync(path.join(root, "src"), out, { recursive: true });
  fs.cpSync(path.join(root, "_locales"), path.join(out, "_locales"), { recursive: true });
  fs.cpSync(path.join(root, "icons"), path.join(out, "icons"), { recursive: true });
  fs.writeFileSync(path.join(out, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
  console.log(`built ${path.relative(root, out)}`);
}
