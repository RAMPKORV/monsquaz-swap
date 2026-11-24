#!/usr/bin/env node
// Bump the application version in one command.
// Usage: node scripts/version.mjs 0.2.0
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
  console.error("usage: node scripts/version.mjs X.Y.Z[-pre]");
  process.exit(1);
}

function updateJson(file) {
  const data = JSON.parse(fs.readFileSync(file, "utf8"));
  data.version = version;
  fs.writeFileSync(file, JSON.stringify(data, null, 2) + "\n");
}

updateJson("package.json");
updateJson("ui/package.json");

// Keep the Cargo workspace version in sync. Do not touch dependency versions.
const cargoPath = "Cargo.toml";
const cargo = fs.readFileSync(cargoPath, "utf8");
let inWorkspacePackage = false;
const updated = cargo.split("\n").map((line) => {
  if (line.trim() === "[workspace.package]") {
    inWorkspacePackage = true;
    return line;
  }
  if (line.startsWith("[") && line.trim() !== "[workspace.package]") {
    inWorkspacePackage = false;
    return line;
  }
  if (inWorkspacePackage && /^version\s*=/.test(line)) {
    return `version = "${version}"`;
  }
  return line;
});
fs.writeFileSync(cargoPath, updated.join("\n"));

console.log(`version -> ${version}`);
console.log("Run `cargo check --workspace` to refresh Cargo.lock.");
