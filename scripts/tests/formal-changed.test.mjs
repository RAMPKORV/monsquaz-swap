import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { touchedModels } from "../formal/changed.mjs";

const mapping = JSON.parse(readFileSync(new URL("../../specs/tla/mapping.json", import.meta.url), "utf8"));

test("an engine change runs the claim model", () => {
  assert.ok(touchedModels(mapping, ["crates/swap-engine/src/engine.rs"]).includes("SwapClaim"));
});

test("a delivery change runs the delivery model only", () => {
  assert.deepEqual(touchedModels(mapping, ["crates/swap-engine/src/delivery.rs"]), ["SwapDelivery"]);
});

test("a model or script change always runs", () => {
  assert.deepEqual(touchedModels(mapping, ["specs/tla/SwapClaim.tla"]), ["always", "SwapClaim"]);
  assert.deepEqual(touchedModels(mapping, ["scripts/formal/check.sh"]), ["always"]);
});

test("a docs change does not run the formal gate", () => {
  assert.deepEqual(touchedModels(mapping, ["docs/PRD.md", "ui/src/App.tsx"]), []);
});

test("every mapped path exists", async () => {
  const { existsSync } = await import("node:fs");
  for (const model of mapping.models) {
    for (const path of [model.spec, ...model.configs, ...model.covers.map((cover) => cover.path)]) {
      assert.ok(existsSync(new URL(`../../${path}`, import.meta.url)), path);
    }
  }
});
