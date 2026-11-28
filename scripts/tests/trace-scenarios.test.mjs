import assert from "node:assert/strict";
import { test } from "node:test";
import { cucumberFeatures, evaluate, findMappings, loadExempt, parseFeature } from "../trace-scenarios.mjs";

const FEATURE = [
  "@v1 @demo",
  "Feature: Demo",
  "",
  "  Scenario: Mapped one",
  "    Given a state",
  "",
  "  @pending",
  "  Scenario: Not built",
  "    Given a state",
].join("\n");

const run = (mappings, extra = {}) =>
  evaluate({
    scenarios: parseFeature(FEATURE, "specs/features/demo.feature"),
    mappings,
    cucumber: [],
    exempt: new Set(),
    ...extra,
  });

test("feature tags and scenario tags combine", () => {
  const [first, second] = parseFeature(FEATURE, "f");
  assert.deepEqual(first.tags, ["@v1", "@demo"]);
  assert.deepEqual(second.tags, ["@v1", "@demo", "@pending"]);
});

test("a mapped scenario and a pending scenario pass", () => {
  const { rows, errors } = run([{ name: "Mapped one", file: "t.rs", line: 1 }]);
  assert.deepEqual(errors, []);
  assert.deepEqual(rows.map((row) => row.status), ["mapped", "pending"]);
});

test("an unmapped scenario without @pending fails", () => {
  assert.match(run([]).errors[0], /no test mapping/);
});

test("a pending scenario with a mapping fails", () => {
  const { errors } = run([
    { name: "Mapped one", file: "t.rs", line: 1 },
    { name: "Not built", file: "t.rs", line: 5 },
  ]);
  assert.match(errors[0], /@pending but has a mapping/);
});

test("a mapping to an unknown scenario fails", () => {
  const { errors } = run([
    { name: "Mapped one", file: "t.rs", line: 1 },
    { name: "Typo", file: "t.rs", line: 9 },
  ]);
  assert.match(errors[0], /no scenario named "Typo"/);
});

test("a cucumber feature counts as mapped in full", () => {
  const { errors } = run([], { cucumber: ["specs/features/demo.feature"] });
  assert.match(errors[0], /@pending but has a mapping/);
});

test("an exempt feature reports unmapped scenarios without failing", () => {
  const { rows, errors } = run([], { exempt: new Set(["specs/features/demo.feature"]) });
  assert.deepEqual(errors, []);
  assert.equal(rows[0].status, "exempt");
});

test("a scenario comment must sit directly above a test", () => {
  const rust = "// scenario: Mapped one\n#[test]\nfn works() {}\n// scenario: Lost\nlet x = 1;\n";
  const { mappings, errors } = findMappings(rust, "t.rs");
  assert.equal(mappings.length, 2);
  assert.equal(errors.length, 1);
  const ts = '  // scenario: Mapped one\n  it("works", () => {});\n';
  assert.deepEqual(findMappings(ts, "t.test.ts").errors, []);
});

test("a doc comment and another attribute between the mapping and the test pass", () => {
  const rust = "    // scenario: Mapped one\n    /// Explains the test.\n    #[allow(clippy::too_many_lines)]\n    #[tokio::test]\n    async fn works() {}\n";
  assert.deepEqual(findMappings(rust, "t.rs").errors, []);
});

test("a mapping above a cfg(test) module fails", () => {
  const rust = "// scenario: Mapped one\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn works() {}\n}\n";
  assert.equal(findMappings(rust, "t.rs").errors.length, 1);
});

test("a mapping above a plain function without a test attribute fails", () => {
  assert.equal(findMappings("// scenario: Mapped one\nfn helper() {}\n", "t.rs").errors.length, 1);
});

test("a mapping above a proptest block passes", () => {
  assert.deepEqual(findMappings("// scenario: Mapped one\nproptest! {\n", "t.rs").errors, []);
});

test("tags on a Rule line apply to its scenarios", () => {
  const feature = "Feature: F\n\n  @pending\n  Rule: R\n\n    Scenario: Inside\n      Given x\n";
  assert.deepEqual(parseFeature(feature, "f")[0].tags, ["@pending"]);
});

test("text inside a doc string is not a scenario", () => {
  const feature = 'Feature: F\n  Scenario: Real\n    Given a note\n      """\n      Scenario: Fake\n      """\n';
  assert.deepEqual(parseFeature(feature, "f").map((s) => s.name), ["Real"]);
});

test("the cucumber runner names its feature files", () => {
  const source = 'SwapWorld::run("../../specs/features/core_invariants.feature").await;';
  assert.deepEqual(cucumberFeatures(source), ["specs/features/core_invariants.feature"]);
});

test("an exemption needs a reason and an unexpired issue or expiry", () => {
  const { exempt, errors } = loadExempt(
    {
      features: [
        { file: "a", reason: "r", expires: "2030-01-01" },
        { file: "b", reason: "r", expires: "2020-01-01" },
        { file: "c" },
      ],
    },
    "2026-01-01",
  );
  assert.deepEqual([...exempt], ["a"]);
  assert.equal(errors.length, 2);
});
