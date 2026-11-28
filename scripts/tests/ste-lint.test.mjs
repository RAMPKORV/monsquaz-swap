import assert from "node:assert/strict";
import { test } from "node:test";
import { compare, glossarySynonyms, lintText, markdownUnits, strip } from "../ste-lint.mjs";

const rules = (findings) => findings.map((finding) => `${finding.rule}:${finding.detail}`);

test("a banned word in a Gherkin step is a finding", () => {
  const findings = lintText("    Then the bot should send the file\n", { gherkin: true });
  assert.deepEqual(rules(findings), ["banned:should"]);
});

test("a banned word inside a code span is ignored", () => {
  assert.deepEqual(lintText("Run `etc.` and `should`.\n"), []);
});

test("a contraction is a finding, a possessive is not", () => {
  assert.deepEqual(rules(lintText("The host can't skip.\n")), ["contraction:can't"]);
  assert.deepEqual(lintText("The participant's track stays hidden.\n"), []);
});

test("a Gherkin step above 20 words is a finding", () => {
  const step = `    When ${"word ".repeat(21).trim()}\n`;
  assert.deepEqual(rules(lintText(step, { gherkin: true })).map((rule) => rule.split(":")[0]), ["length-20"]);
});

test("a description sentence of 25 words passes and 26 words fails", () => {
  assert.deepEqual(lintText(`${"word ".repeat(25).trim()}.\n`), []);
  assert.equal(lintText(`${"word ".repeat(26).trim()}.\n`)[0].rule, "length-25");
});

test("semicolons and colons do not end a sentence", () => {
  const chained = `${Array.from({ length: 6 }, () => "the host sets one value").join("; ")}.\n`;
  const findings = lintText(chained);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].rule, "length-25");
});

test("a numbered list item uses the procedure limit", () => {
  const findings = lintText(`1. ${"word ".repeat(21).trim()}.\n`);
  assert.equal(findings[0].rule, "length-20");
});

test("code blocks and one-token table cells are ignored", () => {
  const source = "```\nyou should not lint this etc.\n```\n\n| should | `etc.` |\n|---|---|\n";
  assert.deepEqual(lintText(source), []);
});

test("URLs are ignored", () => {
  assert.equal(strip("See https://example.com/should for more.").includes("should"), false);
});

test("the glossary table yields standalone synonyms", () => {
  const ste = [
    "## Project terminology",
    "",
    "| Use | Do not use |",
    "|---|---|",
    "| participant | user, player, member (unless you mean a guild member) |",
    "| track | song, file, tune |",
  ].join("\n");
  const synonyms = glossarySynonyms(ste);
  assert.deepEqual(synonyms, ["player", "song", "tune", "user"]);
  assert.deepEqual(rules(lintText("Each user submits a song.\n", { synonyms })), ["synonym:song", "synonym:user"]);
  assert.deepEqual(lintText("The user-facing copy stays short.\n", { synonyms }), []);
});

test("a quoted UI string in a Gherkin step is ignored", () => {
  assert.deepEqual(lintText('    Then the bot shows "you should claim"\n', { gherkin: true }), []);
});

test("markdown paragraphs join wrapped lines", () => {
  const units = markdownUnits("One line\nwraps here.\n\n- item one\n");
  assert.deepEqual(units.map((unit) => unit.text), ["One line wraps here.", "item one"]);
});

test("the baseline accepts a known finding and refuses a new or worse one", () => {
  const finding = { rule: "banned", detail: "should", line: 1, sample: "x" };
  const baseline = { entries: { "a.md|banned|should": { count: 1, reason: "kept", issue: "MSW-1" } } };
  assert.deepEqual(compare([{ file: "a.md", finding }], baseline, "2026-01-01").violations, []);
  const worse = compare([{ file: "a.md", finding }, { file: "a.md", finding }], baseline, "2026-01-01");
  assert.equal(worse.violations.length, 1);
  const fresh = compare([{ file: "b.md", finding }], baseline, "2026-01-01");
  assert.match(fresh.violations[0], /^new finding/);
});

test("a baseline entry without a reason or with an old expiry fails", () => {
  const finding = { rule: "banned", detail: "should", line: 1, sample: "x" };
  const noReason = { entries: { "a.md|banned|should": { count: 1, issue: "MSW-1" } } };
  assert.equal(compare([{ file: "a.md", finding }], noReason, "2026-01-01").violations.length, 1);
  const expired = { entries: { "a.md|banned|should": { count: 1, reason: "r", expires: "2025-01-01" } } };
  assert.equal(compare([{ file: "a.md", finding }], expired, "2026-01-01").violations.length, 1);
});
