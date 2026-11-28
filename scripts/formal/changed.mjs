#!/usr/bin/env node
// Exit 0 when a changed file is covered by a TLA+ model in specs/tla/mapping.json.
// Exit 1 when no mapped file changed. Exit 2 on a usage or git error.
//
// Usage:
//   node scripts/formal/changed.mjs <base-ref>        # git diff <base>...HEAD
//   node scripts/formal/changed.mjs --files a.rs b.rs # explicit list (tests)
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");

export function touchedModels(mapping, files) {
  const hits = new Set();
  for (const file of files) {
    for (const prefix of mapping.always ?? []) {
      if (file.startsWith(prefix)) hits.add("always");
    }
    for (const model of mapping.models ?? []) {
      const paths = [model.spec, ...(model.configs ?? []), ...(model.covers ?? []).map((c) => c.path)];
      if (paths.some((path) => file === path || (path.endsWith("/") && file.startsWith(path)))) {
        hits.add(model.name);
      }
    }
  }
  return [...hits];
}

function main(argv) {
  let mapping;
  try {
    mapping = JSON.parse(readFileSync(join(root, "specs/tla/mapping.json"), "utf8"));
  } catch (error) {
    console.error(`formal: cannot read specs/tla/mapping.json: ${error.message}`);
    return 2;
  }
  let files;
  if (argv[0] === "--files") {
    files = argv.slice(1);
  } else if (argv.length === 1) {
    try {
      files = execFileSync("git", ["diff", "--name-only", "--no-renames", `${argv[0]}...HEAD`], {
        cwd: root,
        encoding: "utf8",
      })
        .split("\n")
        .filter(Boolean);
    } catch (error) {
      console.error(`formal: git diff failed: ${error.message}`);
      return 2;
    }
  } else {
    console.error("usage: changed.mjs <base-ref> | --files <path>...");
    return 2;
  }
  const hits = touchedModels(mapping, files);
  if (hits.length > 0) {
    console.log(`formal: mapped change (${hits.join(", ")})`);
    return 0;
  }
  console.log("formal: no mapped file changed");
  return 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main(process.argv.slice(2)));
}
