// Usage: node scripts/set-version.mjs 0.2.0
// Updates the version everywhere it is declared, so a release tag matches the builds.
import fs from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
  console.error("Usage: node scripts/set-version.mjs <major.minor.patch>");
  process.exit(1);
}

const json = (file, update) => {
  const data = JSON.parse(fs.readFileSync(file, "utf8"));
  update(data);
  fs.writeFileSync(file, JSON.stringify(data, null, 2) + "\n");
};

json("app/package.json", (d) => (d.version = version));
json("app/src-tauri/tauri.conf.json", (d) => (d.version = version));
json("extension/package.json", (d) => (d.version = version));
json("extension/manifest.base.json", (d) => (d.version = version));

const cargo = "app/src-tauri/Cargo.toml";
fs.writeFileSync(
  cargo,
  fs.readFileSync(cargo, "utf8").replace(/^version = ".*"$/m, `version = "${version}"`),
);

console.log(`Version set to ${version}. Commit, then: git tag v${version} && git push --tags`);
