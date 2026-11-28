#!/usr/bin/env node
// Gherkin traceability gate (D-038).
//
// Every scenario in specs/features must either:
//   - have a test that declares it with `// scenario: <exact name>` directly
//     above the test, or
//   - live in a feature file that cucumber-rs executes, or
//   - carry the @pending tag.
//
// The gate fails when:
//   - a scenario without @pending has no mapping;
//   - a @pending scenario has a mapping;
//   - a mapping names a scenario that does not exist;
//   - two scenarios have the same name;
//   - a mapping comment is not directly above a test.
//
// It writes docs/generated/SCENARIO_COVERAGE.md.
// Usage: node scripts/trace-scenarios.mjs [--check]   (--check: do not write)
import { readFileSync, readdirSync, statSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const FEATURES = "specs/features";
const TEST_ROOTS = ["crates", "src-tauri/src", "ui/src", "ui/e2e"];
const CUCUMBER_RUNNERS = ["crates/swap-acceptance/tests/cucumber.rs"];
const BASELINE = "scripts/trace-scenarios.baseline.json";
const REPORT = "docs/generated/SCENARIO_COVERAGE.md";
const MAPPING_RE = /^\s*\/\/\s*scenario:\s*(.+?)\s*$/;
// After the mapping comments, the scanner skips blank lines, comments, doc
// comments, and non-test attributes. The next line must start a real test.
const TEST_START_RE =
  /^\s*(#\[test\]|#\[tokio::test|#\[proptest\]|proptest!\s*\{|(it|test)(\.each\([^)]*\))?\()/;
const SKIP_RE = /^\s*(\/\/|#\[|$)/;

/** Parse one feature file into scenarios with their effective tags. */
export function parseFeature(text, file) {
  const scenarios = [];
  let featureTags = [];
  let ruleTags = [];
  let pendingTags = [];
  let seenFeature = false;
  let inDocString = false;
  text.split("\n").forEach((raw, index) => {
    const line = raw.trim();
    if (line.startsWith('"""') || line.startsWith("```")) {
      inDocString = !inDocString;
      return;
    }
    if (inDocString || line.startsWith("#") || line === "") return;
    if (line.startsWith("@")) {
      pendingTags.push(...line.split(/\s+/).filter((tag) => tag.startsWith("@")));
      return;
    }
    const feature = line.match(/^Feature:/);
    if (feature && !seenFeature) {
      seenFeature = true;
      featureTags = pendingTags;
      pendingTags = [];
      return;
    }
    if (/^Rule:/.test(line)) {
      ruleTags = pendingTags;
      pendingTags = [];
      return;
    }
    const scenario = line.match(/^Scenario(?: Outline)?:\s*(.+)$/);
    if (scenario) {
      scenarios.push({
        name: scenario[1].trim(),
        file,
        line: index + 1,
        tags: [...featureTags, ...ruleTags, ...pendingTags],
      });
    }
    pendingTags = [];
  });
  return scenarios;
}

/** Find `// scenario:` comments in one source file. */
export function findMappings(text, file) {
  const mappings = [];
  const errors = [];
  const lines = text.split("\n");
  lines.forEach((line, index) => {
    const match = line.match(MAPPING_RE);
    if (!match) return;
    let next = index + 1;
    while (next < lines.length && !TEST_START_RE.test(lines[next]) && SKIP_RE.test(lines[next])) next += 1;
    if (next >= lines.length || !TEST_START_RE.test(lines[next])) {
      errors.push(`${file}:${index + 1}: scenario comment is not directly above a test`);
    }
    mappings.push({ name: match[1], file, line: index + 1 });
  });
  return { mappings, errors };
}

/** Feature files that a cucumber runner executes in full. */
export function cucumberFeatures(text) {
  const features = [];
  for (const match of text.matchAll(/run\(\s*"([^"]+\.feature)"/g)) {
    const name = match[1].split("/").pop();
    features.push(`${FEATURES}/${name}`);
  }
  return features;
}

/** Apply the rules. Returns { rows, errors }. */
export function evaluate({ scenarios, mappings, cucumber, exempt }) {
  const errors = [];
  const byName = new Map();
  for (const scenario of scenarios) {
    if (byName.has(scenario.name)) {
      errors.push(`duplicate scenario name "${scenario.name}" in ${scenario.file} and ${byName.get(scenario.name).file}`);
    }
    byName.set(scenario.name, scenario);
  }
  const mapped = new Map();
  for (const mapping of mappings) {
    if (!byName.has(mapping.name)) {
      errors.push(`${mapping.file}:${mapping.line}: no scenario named "${mapping.name}"`);
      continue;
    }
    if (!mapped.has(mapping.name)) mapped.set(mapping.name, []);
    mapped.get(mapping.name).push(`${mapping.file}:${mapping.line}`);
  }
  const rows = scenarios.map((scenario) => {
    const pending = scenario.tags.includes("@pending");
    const byCucumber = cucumber.includes(scenario.file);
    const tests = mapped.get(scenario.name) ?? [];
    let status;
    if (pending && (tests.length > 0 || byCucumber)) {
      status = "error";
      errors.push(`${scenario.file}:${scenario.line}: "${scenario.name}" is @pending but has a mapping`);
    } else if (pending) {
      status = "pending";
    } else if (byCucumber || tests.length > 0) {
      status = "mapped";
    } else if (exempt.has(scenario.file)) {
      status = "exempt";
    } else {
      status = "error";
      errors.push(`${scenario.file}:${scenario.line}: "${scenario.name}" has no test mapping and no @pending tag`);
    }
    return { ...scenario, status, tests, byCucumber };
  });
  return { rows, errors };
}

/** Exempt feature files from the baseline. Every entry needs a reason and an issue or an expiry. */
export function loadExempt(baseline, today) {
  const exempt = new Set();
  const errors = [];
  for (const entry of baseline.features ?? []) {
    if (!entry.file || !entry.reason || !(entry.issue || entry.expires)) {
      errors.push(`baseline entry ${JSON.stringify(entry)} needs file, reason, and issue or expires`);
      continue;
    }
    if (entry.expires && entry.expires < today) {
      errors.push(`baseline entry for ${entry.file} expired on ${entry.expires}`);
      continue;
    }
    exempt.add(entry.file);
  }
  return { exempt, errors };
}

export function renderReport(rows) {
  const features = [...new Set(rows.map((row) => row.file))].sort();
  const count = (list, status) => list.filter((row) => row.status === status).length;
  const lines = [
    "# Scenario coverage",
    "",
    "> <!-- GENERATED FILE — run `node scripts/trace-scenarios.mjs`; do not edit by hand. -->",
    "",
    `Total: ${rows.length} scenarios. Mapped: ${count(rows, "mapped")}. Pending: ${count(rows, "pending")}. Exempt: ${count(rows, "exempt")}.`,
    "",
    "| Feature | Scenarios | Mapped | Pending | Exempt |",
    "|---|---|---|---|---|",
  ];
  for (const feature of features) {
    const list = rows.filter((row) => row.file === feature);
    const name = feature.replace(`${FEATURES}/`, "");
    const note = list.some((row) => row.byCucumber) ? " (cucumber)" : "";
    lines.push(`| \`${name}\`${note} | ${list.length} | ${count(list, "mapped")} | ${count(list, "pending")} | ${count(list, "exempt")} |`);
  }
  lines.push("", "## Unmapped scenarios", "");
  for (const feature of features) {
    const list = rows.filter((row) => row.file === feature && row.status !== "mapped");
    if (list.length === 0) continue;
    lines.push(`### \`${feature.replace(`${FEATURES}/`, "")}\``, "");
    for (const row of list) lines.push(`- ${row.name} (${row.status})`);
    lines.push("");
  }
  return `${lines.join("\n").trimEnd()}\n`;
}

function walk(dir, out = []) {
  if (!existsSync(dir)) return out;
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "target" || name === "dist" || name.startsWith(".")) continue;
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else if (/\.(rs|ts|tsx)$/.test(name)) out.push(path);
  }
  return out;
}

function main(argv) {
  const check = argv.includes("--check");
  const featureDir = join(root, FEATURES);
  const scenarios = readdirSync(featureDir)
    .filter((name) => name.endsWith(".feature"))
    .sort()
    .flatMap((name) => parseFeature(readFileSync(join(featureDir, name), "utf8"), `${FEATURES}/${name}`));

  const errors = [];
  const mappings = [];
  for (const testRoot of TEST_ROOTS) {
    for (const path of walk(join(root, testRoot))) {
      const found = findMappings(readFileSync(path, "utf8"), relative(root, path));
      mappings.push(...found.mappings);
      errors.push(...found.errors);
    }
  }
  const cucumber = CUCUMBER_RUNNERS.flatMap((runner) =>
    existsSync(join(root, runner)) ? cucumberFeatures(readFileSync(join(root, runner), "utf8")) : [],
  );
  const baseline = existsSync(join(root, BASELINE)) ? JSON.parse(readFileSync(join(root, BASELINE), "utf8")) : {};
  const today = new Date().toISOString().slice(0, 10);
  const { exempt, errors: baselineErrors } = loadExempt(baseline, today);
  errors.push(...baselineErrors);

  const result = evaluate({ scenarios, mappings, cucumber, exempt });
  errors.push(...result.errors);
  const report = renderReport(result.rows);
  if (!check) writeFileSync(join(root, REPORT), report);

  const count = (status) => result.rows.filter((row) => row.status === status).length;
  console.log(
    `scenarios: ${result.rows.length} total, ${count("mapped")} mapped, ${count("pending")} pending, ${count("exempt")} exempt`,
  );
  if (check) {
    if (!existsSync(join(root, REPORT))) {
      errors.push(`${REPORT} is missing; run node scripts/trace-scenarios.mjs`);
    } else if (readFileSync(join(root, REPORT), "utf8") !== report) {
      errors.push(`${REPORT} is stale; run node scripts/trace-scenarios.mjs`);
    }
  }
  for (const error of errors) console.error(`trace: ${error}`);
  return errors.length === 0 ? 0 : 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main(process.argv.slice(2)));
}
