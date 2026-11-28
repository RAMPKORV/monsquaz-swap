#!/usr/bin/env node
// ASD-STE100 heuristic linter (D-038). This is a heuristic, not a parser of
// English. It checks the rules in ai-rules/STE.md that a regular expression
// can check:
//   - banned words (should, etc., e.g., i.e., and/or, appropriate, ...);
//   - contractions;
//   - sentence length (20 words for Gherkin steps and procedure lists,
//     25 words for descriptions);
//   - glossary synonyms from the terminology table in ai-rules/STE.md.
// It ignores code blocks, code spans, URLs, quoted UI strings in Gherkin, and
// table cells that hold one token.
//
// Gated files fail on a finding that is not in scripts/ste-lint.baseline.json
// or that occurs more often than the baseline allows. Advisory files only
// print a summary.
//
// Usage:
//   node scripts/ste-lint.mjs                  # gate + advisory summary
//   node scripts/ste-lint.mjs --verbose        # also list advisory findings
//   node scripts/ste-lint.mjs --update-baseline --baseline-issue MSW-###
import { readFileSync, readdirSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const BASELINE = "scripts/ste-lint.baseline.json";
const PROCEDURE_LIMIT = 20;
const DESCRIPTION_LIMIT = 25;

export const BANNED = [
  ["should", /\bshould\b/i],
  ["etc.", /\betc\b\.?/i],
  ["e.g.", /\be\.g\./i],
  ["i.e.", /\bi\.e\./i],
  ["and/or", /\band\/or\b/i],
  ["appropriate", /\bappropriate\b/i],
  ["reasonable", /\breasonable\b/i],
  ["as needed", /\bas needed\b/i],
  ["robust", /\brobust\b/i],
];

const CONTRACTION =
  /\b(?:\w+n['’]t|(?:i|you|we|they|it|that|there|what|here|let|who)['’](?:s|re|ve|ll|d|m)|i['’]m|\w+['’](?:re|ve|ll))\b/i;

// Words from the "Do not use" column that also have a valid project meaning.
// The table itself allows them in a stated context.
const SYNONYM_ALLOW = new Set([
  "project", "run", "file", "assignment", "accept", "take", "pass",
  "release", "history", "backup", "owner", "member", "replacement",
]);

/** Read the "Do not use" column of the terminology table in STE.md. */
export function glossarySynonyms(steText) {
  const start = steText.indexOf("## Project terminology");
  if (start < 0) return [];
  const words = new Set();
  for (const line of steText.slice(start).split("\n")) {
    const cells = line.split("|").map((cell) => cell.trim());
    if (cells.length < 4 || cells[1] === "Use" || cells[1].startsWith("---")) continue;
    const avoid = cells[2].replace(/\([^)]*\)/g, "");
    for (const term of avoid.split(",")) {
      const word = term.trim().toLowerCase();
      if (word && !SYNONYM_ALLOW.has(word)) words.add(word);
    }
  }
  return [...words].sort();
}

function synonymRegex(word) {
  const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&").replace(/ /g, "\\s+");
  // A standalone word: not part of a hyphenated or slashed compound.
  return new RegExp(`(?<![\\w\\-/])${escaped}(?:s|es|ed|ing)?(?![\\w\\-/])`, "i");
}

/** Remove code spans, URLs, autolinks, and (optionally) double-quoted strings. */
export function strip(text, { quotes = false } = {}) {
  let out = text
    .replace(/`[^`]*`/g, " CODE ")
    .replace(/<https?:[^>]*>/g, " URL ")
    .replace(/\]\([^)]*\)/g, "]")
    .replace(/https?:\/\/\S+/g, " URL ");
  if (quotes) out = out.replace(/"[^"]*"/g, " QUOTE ");
  return out;
}

export function wordCount(sentence) {
  return sentence.split(/\s+/).filter((token) => /[A-Za-z0-9]/.test(token)).length;
}

export function sentences(text) {
  return text
    .split(/(?<=[.!?;:])\s+(?=[A-Z0-9`"(*])/)
    .map((part) => part.trim())
    .filter(Boolean);
}

/** Split a Markdown file into lintable units: { text, kind, line }. */
export function markdownUnits(source) {
  const units = [];
  const lines = source.split("\n");
  let inFence = false;
  let paragraph = null;
  const flush = () => {
    if (paragraph) units.push(paragraph);
    paragraph = null;
  };
  lines.forEach((raw, index) => {
    const line = raw.trimEnd();
    if (/^\s*(```|~~~)/.test(line)) {
      flush();
      inFence = !inFence;
      return;
    }
    if (inFence) return;
    if (line.trim() === "" || /^\s*(<!--|>?\s*<!--)/.test(line)) {
      flush();
      return;
    }
    if (/^\s*#{1,6}\s/.test(line)) {
      flush();
      units.push({ text: line.replace(/^\s*#+\s*/, ""), kind: "heading", line: index + 1 });
      return;
    }
    if (/^\s*\|/.test(line)) {
      flush();
      if (/^\s*\|[\s:|-]+\|\s*$/.test(line)) return;
      for (const cell of line.split("|").slice(1, -1)) {
        const text = cell.trim();
        // A cell with one token is an identifier, a keyword, or a value.
        if (strip(text).trim().split(/\s+/).length <= 1) continue;
        units.push({ text, kind: "description", line: index + 1 });
      }
      return;
    }
    const item = line.match(/^(\s*)([-*+]|\d+\.)\s+(.*)$/);
    if (item) {
      flush();
      paragraph = { text: item[3], kind: /\d+\./.test(item[2]) ? "procedure" : "description", line: index + 1 };
      return;
    }
    if (paragraph) paragraph.text += ` ${line.trim()}`;
    else paragraph = { text: line.trim().replace(/^>\s*/, ""), kind: "description", line: index + 1 };
  });
  flush();
  return units;
}

/** Split a Gherkin file into lintable units. */
export function gherkinUnits(source) {
  const units = [];
  source.split("\n").forEach((raw, index) => {
    const line = raw.trim();
    if (line === "" || line.startsWith("#") || line.startsWith("@") || line.startsWith("|")) return;
    const step = line.match(/^(Given|When|Then|And|But)\s+(.*)$/);
    if (step) {
      units.push({ text: step[2], kind: "step", line: index + 1 });
      return;
    }
    const header = line.match(/^(Feature|Rule|Background|Scenario Outline|Scenario|Examples):\s*(.*)$/);
    units.push({ text: header ? header[2] : line, kind: "description", line: index + 1 });
  });
  return units;
}

/** Lint one file. Returns findings: { rule, detail, line, sample }. */
export function lintText(source, { gherkin = false, synonyms = [] } = {}) {
  const units = gherkin ? gherkinUnits(source) : markdownUnits(source);
  const synonymRules = synonyms.map((word) => [word, synonymRegex(word)]);
  const findings = [];
  for (const unit of units) {
    const text = strip(unit.text, { quotes: gherkin });
    for (const [word, pattern] of BANNED) {
      if (pattern.test(text)) findings.push({ rule: "banned", detail: word, line: unit.line, sample: unit.text });
    }
    const contraction = text.match(CONTRACTION);
    if (contraction) {
      findings.push({ rule: "contraction", detail: contraction[0].toLowerCase(), line: unit.line, sample: unit.text });
    }
    for (const [word, pattern] of synonymRules) {
      if (pattern.test(text)) findings.push({ rule: "synonym", detail: word, line: unit.line, sample: unit.text });
    }
    if (unit.kind === "heading") continue;
    const limit = unit.kind === "description" ? DESCRIPTION_LIMIT : PROCEDURE_LIMIT;
    const parts = unit.kind === "step" ? [text] : sentences(text);
    for (const sentence of parts) {
      const count = wordCount(sentence);
      if (count > limit) {
        findings.push({
          rule: `length-${limit}`,
          detail: sentence.split(/\s+/).slice(0, 8).join(" "),
          line: unit.line,
          sample: `${count} words: ${sentence}`,
        });
      }
    }
  }
  return findings;
}

export function findingKey(file, finding) {
  return `${file}|${finding.rule}|${finding.detail}`;
}

/** Compare gated findings with the baseline. Returns a list of violations. */
export function compare(findings, baseline, today) {
  const counts = new Map();
  for (const { file, finding } of findings) {
    const key = findingKey(file, finding);
    const entry = counts.get(key) ?? { count: 0, file, finding };
    entry.count += 1;
    counts.set(key, entry);
  }
  const violations = [];
  for (const [key, { count, file, finding }] of counts) {
    const entry = baseline.entries?.[key];
    const where = `${file}:${finding.line} [${finding.rule}] ${finding.sample}`;
    if (!entry) {
      violations.push(`new finding: ${where}`);
      continue;
    }
    if (count > (entry.count ?? 1)) violations.push(`count ${count} > baseline ${entry.count}: ${where}`);
    const hasIssue = Boolean(entry.issue) && entry.issue !== "UNASSIGNED";
    if (!entry.reason || (!hasIssue && !entry.expires)) {
      violations.push(`baseline entry needs a reason and an issue or expiry: ${key}`);
    }
    if (entry.expires && entry.expires < today) violations.push(`baseline entry expired ${entry.expires}: ${key}`);
  }
  return { counts, violations };
}

function listFiles() {
  const features = readdirSync(join(root, "specs/features"))
    .filter((name) => name.endsWith(".feature"))
    .sort()
    .map((name) => `specs/features/${name}`);
  const rules = readdirSync(join(root, "ai-rules"))
    .filter((name) => name.endsWith(".md"))
    .sort()
    .map((name) => `ai-rules/${name}`);
  return {
    gated: [...features, "docs/EVENT_SETTINGS.md"],
    advisory: ["docs/PRD.md", "docs/RUNBOOK.md", ...rules].filter((file) => existsSync(join(root, file))),
  };
}

function main(argv) {
  const verbose = argv.includes("--verbose");
  const update = argv.includes("--update-baseline");
  const issueIndex = argv.indexOf("--baseline-issue");
  const issue = issueIndex >= 0 ? argv[issueIndex + 1] : "UNASSIGNED";
  const synonyms = glossarySynonyms(readFileSync(join(root, "ai-rules/STE.md"), "utf8"));
  const { gated, advisory } = listFiles();
  const lint = (file) =>
    lintText(readFileSync(join(root, file), "utf8"), { gherkin: file.endsWith(".feature"), synonyms }).map(
      (finding) => ({ file, finding }),
    );

  const gatedFindings = gated.flatMap(lint);
  const baselinePath = join(root, BASELINE);
  const baseline = existsSync(baselinePath) ? JSON.parse(readFileSync(baselinePath, "utf8")) : { entries: {} };
  const today = new Date().toISOString().slice(0, 10);
  const { counts, violations } = compare(gatedFindings, baseline, today);

  if (update) {
    const entries = {};
    for (const [key, { count }] of [...counts].sort(([a], [b]) => a.localeCompare(b))) {
      const previous = baseline.entries?.[key] ?? {};
      entries[key] = {
        count,
        reason: previous.reason ?? "",
        issue: previous.issue ?? issue,
        expires: previous.expires ?? null,
      };
    }
    const note = "Reviewed STE debt. Each entry needs a reason and an issue or an expiry.";
    writeFileSync(baselinePath, `${JSON.stringify({ version: 1, note, entries }, null, 2)}\n`);
    console.log(`ste: baseline updated with ${Object.keys(entries).length} entries`);
    return 0;
  }

  const advisoryFindings = advisory.flatMap(lint);
  const byRule = {};
  for (const { finding } of advisoryFindings) byRule[finding.rule] = (byRule[finding.rule] ?? 0) + 1;
  console.log(`ste: ${advisoryFindings.length} advisory findings in ${advisory.length} files (not blocking)`);
  for (const [rule, count] of Object.entries(byRule).sort()) console.log(`  advisory ${rule}: ${count}`);
  if (verbose) {
    for (const { file, finding } of advisoryFindings) {
      console.log(`  ${file}:${finding.line} [${finding.rule}] ${finding.detail}`);
    }
  }

  if (violations.length === 0) {
    console.log(`ste: pass. ${gated.length} gated files; ${counts.size} findings covered by the baseline.`);
    return 0;
  }
  console.error(`ste: fail. ${violations.length} gated findings need action.`);
  for (const violation of violations) console.error(`  ${violation}`);
  console.error("ste: rewrite the sentence in STE, or add a reviewed baseline entry with a reason.");
  return 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main(process.argv.slice(2)));
}
