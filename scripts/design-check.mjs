#!/usr/bin/env node
// Deterministic design gate.
//
// 1. Refract renders the page matrix at the enforced viewports and returns
//    structured overflow/clipping findings.
// 2. A Playwright pass checks contrast and keyboard focus.
// 3. A CSS pass checks tokens, !important, and the advisory scales.
//
// Locked findings fail. Advisory findings print but do not fail. Existing debt
// lives in design/baseline.json, which must be updated with --update-baseline
// and reviewed.
import fs from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const root = process.cwd();
const args = process.argv.slice(2);
const updateBaseline = args.includes("--update-baseline");
const baselineIssue = argument("--baseline-issue") ?? "UNASSIGNED";
const baseUrl = process.env.DESIGN_BASE_URL ?? "http://127.0.0.1:4199";
const refractCache = process.env.REFRACT_CACHE;
const today = new Date().toISOString().slice(0, 10);

if (!refractCache) {
  fail("REFRACT_CACHE is missing. Run scripts/design-check.sh.");
}

const policy = JSON.parse(await read("design/policy.json"));
const pages = JSON.parse(await read("design/pages.json"));
const baselinePath = "design/baseline.json";
const artifacts = "design/artifacts/refract";
const findings = new Map();

function argument(name) {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

function fail(message) {
  console.error(`design: ${message}`);
  process.exit(2);
}

async function read(file) {
  return fs.readFile(path.join(root, file), "utf8");
}

function ruleStatus(rule) {
  return policy.rules[rule]?.status ?? "advisory";
}

function record(rule, key, sample) {
  const current = findings.get(key) ?? { rule, count: 0, sample };
  current.count += 1;
  findings.set(key, current);
}

function emit(rule, key, sample) {
  record(rule, `${rule}|${key}`, sample);
}

// ---------------------------------------------------------------- static CSS

const cssFiles = [];
async function walk(directory) {
  for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
    if (entry.name === "node_modules" || entry.name === "dist") {
      continue;
    }
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      await walk(full);
    } else if (entry.name.endsWith(".css")) {
      cssFiles.push(full);
    }
  }
}
await walk(path.join(root, "ui/src"));

const tokenRanges = new Map();
function tokenRangesFor(css) {
  const ranges = [];
  const pattern = /(:root|\[data-theme=(?:"|')?(?:light|dark)(?:"|')?\])\s*\{[^}]*\}/g;
  let match;
  while ((match = pattern.exec(css)) !== null) {
    ranges.push([match.index, match.index + match[0].length]);
  }
  return ranges;
}

function lineOf(css, index) {
  return css.slice(0, index).split("\n").length;
}

function stripPixelValues(value) {
  return [...value.matchAll(/(-?\d+(?:\.\d+)?)px/g)].map((match) =>
    Number(match[1]),
  );
}

let hasReducedMotion = false;
let hasFocusVisible = false;
for (const file of cssFiles) {
  const css = await fs.readFile(file, "utf8");
  const relative = path.relative(root, file).replaceAll(path.sep, "/");
  const ranges = tokenRangesFor(css);
  tokenRanges.set(relative, ranges);
  if (css.includes("prefers-reduced-motion")) {
    hasReducedMotion = true;
  }
  if (css.includes(":focus-visible")) {
    hasFocusVisible = true;
  }

  const tokens = [];
  for (const [start, end] of ranges) {
    tokens.push([start, end]);
  }
  const inTokenBlock = (index) =>
    tokens.some(([start, end]) => index >= start && index < end);

  // Raw colors outside token blocks.
  for (const match of css.matchAll(/#[0-9a-fA-F]{3,8}\b|\brgba?\([^)]*\)|\bhsla?\([^)]*\)/g)) {
    if (inTokenBlock(match.index)) {
      continue;
    }
    const sample = `${relative}:${lineOf(css, match.index)} ${match[0]}`;
    emit("raw-color", `${relative}|${match[0]}`, sample);
  }

  // !important. Motion overrides are allowed.
  for (const match of css.matchAll(/!important\b/g)) {
    const line = css.slice(
      css.lastIndexOf("\n", match.index) + 1,
      css.indexOf("\n", match.index) === -1
        ? css.length
        : css.indexOf("\n", match.index),
    );
    const allowed = (policy.rules.important?.allowedProperties ?? []).some(
      (property) => line.includes(property),
    );
    if (allowed) {
      continue;
    }
    emit("important", `${relative}`, `${relative}:${lineOf(css, match.index)}`);
  }

  // Advisory scale declarations.
  const declarations = [
    ["spacing-scale", /(?:padding|margin|gap|row-gap|column-gap)\s*:\s*([^;]+)/g],
    ["type-scale", /font-size\s*:\s*([^;]+)/g],
    ["radius-scale", /border-radius\s*:\s*([^;]+)/g],
  ];
  for (const [rule, pattern] of declarations) {
    for (const match of css.matchAll(pattern)) {
      const scale = policy.rules[rule]?.scale ?? [];
      for (const value of stripPixelValues(match[1])) {
        if (rule === "radius-scale" && value >= 999) {
          continue;
        }
        if (scale.includes(value)) {
          continue;
        }
        emit(
          rule,
          `${relative}|${value}`,
          `${relative}:${lineOf(css, match.index)} ${value}px`,
        );
      }
    }
  }

  for (const match of css.matchAll(/box-shadow\s*:\s*([^;]+)/g)) {
    const value = match[1].trim();
    if (value === "none" || value.includes("var(")) {
      continue;
    }
    emit("shadow-token", `${relative}`, `${relative}:${lineOf(css, match.index)}`);
  }
}

if (!hasReducedMotion) {
  emit("reduced-motion", "ui/src", "no prefers-reduced-motion rule");
}
if (!hasFocusVisible) {
  emit("focus-visible", "ui/src", "no :focus-visible rule");
}

// ------------------------------------------------------------- browser pass

const { chromium } = await import(
  pathToFileURL(path.join(refractCache, "packages/core/node_modules/playwright/index.mjs"))
);
const browser = await chromium.launch({ headless: true });

for (const pageDef of pages.pages) {
  const url = `${baseUrl}/?${pageDef.query}`;

  if (pageDef.contrast) {
    for (const viewport of pages.contrastViewports) {
      const [width, height] = viewport.split("x").map(Number);
      const context = await browser.newContext({
        viewport: { width, height },
        deviceScaleFactor: 1,
      });
      const page = await context.newPage();
      await ready(page, url);
      const contrast = await page.evaluate(contrastInPage);
      const results = contrast.results;
      for (const item of results) {
        emit(
          "color-contrast",
          `${pageDef.id}|${viewport}|${item.selector}`,
          `${pageDef.id} ${viewport} ${item.selector} ratio ${item.ratio} required ${item.required} "${item.text}"`,
        );
      }
      await context.close();
    }
  }

  if (pageDef.focus) {
    for (const viewport of pages.focusViewports) {
      const [width, height] = viewport.split("x").map(Number);
      const context = await browser.newContext({
        viewport: { width, height },
        deviceScaleFactor: 1,
      });
      const page = await context.newPage();
      await ready(page, url);
      const results = await focusInPage(page);
      for (const item of results) {
        emit(
          "focus-visible",
          `${pageDef.id}|${viewport}|${item.selector}`,
          `${pageDef.id} ${viewport} ${item.selector}`,
        );
      }
      await context.close();
    }
  }
}

await browser.close();

async function ready(page, url) {
  await page.goto(url, { waitUntil: "networkidle" });
  await page
    .waitForFunction(
      () => document.querySelector('[data-app-status="ready"]') !== null,
      undefined,
      { timeout: 10_000 },
    )
    .catch(() => undefined);
  await page.waitForTimeout(350);
}

async function focusInPage(page) {
  const results = [];
  await page.keyboard.press("Tab");
  for (let index = 0; index < 14; index += 1) {
    const info = await page.evaluate(() => {
      const element = document.activeElement;
      if (!element || element === document.body) {
        return null;
      }
      const style = getComputedStyle(element);
      const tag = element.tagName.toLowerCase();
      const classes = [...element.classList].slice(0, 2);
      const selector = `${tag}${element.id ? `#${element.id}` : ""}${
        classes.length ? `.${classes.join(".")}` : ""
      }`;
      return {
        selector,
        disabled: "disabled" in element && element.disabled,
        outlineStyle: style.outlineStyle,
        outlineWidth: style.outlineWidth,
        boxShadow: style.boxShadow,
        focusVisible: element.matches(":focus-visible"),
      };
    });
    if (
      info &&
      !info.disabled &&
      info.outlineStyle === "none" &&
      info.boxShadow === "none"
    ) {
      results.push(info);
    }
    await page.keyboard.press("Tab");
  }
  return results;
}

function contrastInPage() {
  const parse = (value) => {
    const match = value.match(/rgba?\(([^)]+)\)/);
    if (!match) {
      return null;
    }
    const parts = match[1].split(/[, /]+/).filter(Boolean).map(Number);
    if (parts.length < 3 || parts.some((part) => Number.isNaN(part))) {
      return null;
    }
    return {
      r: parts[0],
      g: parts[1],
      b: parts[2],
      a: parts.length > 3 ? parts[3] : 1,
    };
  };
  const over = (top, bottom) => {
    const a = top.a + bottom.a * (1 - top.a);
    if (a === 0) {
      return { r: 0, g: 0, b: 0, a: 0 };
    }
    return {
      r: (top.r * top.a + bottom.r * bottom.a * (1 - top.a)) / a,
      g: (top.g * top.a + bottom.g * bottom.a * (1 - top.a)) / a,
      b: (top.b * top.a + bottom.b * bottom.a * (1 - top.a)) / a,
      a,
    };
  };
  const channel = (value) => {
    const scaled = value / 255;
    return scaled <= 0.04045
      ? scaled / 12.92
      : ((scaled + 0.055) / 1.055) ** 2.4;
  };
  const luminance = (color) =>
    0.2126 * channel(color.r) +
    0.7152 * channel(color.g) +
    0.0722 * channel(color.b);
  const ratio = (left, right) => {
    const a = luminance(left);
    const b = luminance(right);
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  };
  const selector = (element) => {
    const tag = element.tagName.toLowerCase();
    const classes = [...element.classList]
      .filter((name) => name !== "visually-hidden")
      .slice(0, 2);
    return `${tag}${element.id ? `#${element.id}` : ""}${
      classes.length ? `.${classes.join(".")}` : ""
    }`;
  };
  const background = (element) => {
    const layers = [];
    let node = element;
    while (node) {
      const style = getComputedStyle(node);
      if (style.backgroundImage !== "none") {
        return null;
      }
      const color = parse(style.backgroundColor);
      if (color && color.a > 0) {
        layers.push(color);
        if (color.a >= 1) {
          break;
        }
      }
      node = node.parentElement;
    }
    let result = { r: 255, g: 255, b: 255, a: 1 };
    for (let index = layers.length - 1; index >= 0; index -= 1) {
      result = over(layers[index], result);
    }
    return result;
  };

  const results = [];
  let withText = 0;
  let skippedBackground = 0;
  const nodes = [...document.querySelectorAll("body *")];
  for (const element of nodes) {
    if (results.length >= 300) {
      break;
    }
    if (element.closest("[aria-hidden='true']")) {
      continue;
    }
    const style = getComputedStyle(element);
    if (
      style.display === "none" ||
      style.visibility === "hidden" ||
      Number(style.opacity) === 0
    ) {
      continue;
    }
    const rect = element.getBoundingClientRect();
    if (rect.width <= 1 || rect.height <= 1) {
      continue;
    }
    const text = [...element.childNodes]
      .filter((node) => node.nodeType === Node.TEXT_NODE)
      .map((node) => node.textContent.trim())
      .join(" ")
      .trim();
    if (text.length === 0) {
      continue;
    }
    withText += 1;
    const foreground = parse(style.color);
    const behind = background(element);
    if (!foreground || !behind) {
      skippedBackground += 1;
      continue;
    }
    const visible = foreground.a < 1 ? over(foreground, behind) : foreground;
    const size = Number.parseFloat(style.fontSize);
    const weight = Number.parseInt(style.fontWeight, 10) || 400;
    const required =
      size >= 24 || (size >= 18.66 && weight >= 700)
        ? 3.0
        : 4.5;
    const measured = ratio(visible, behind);
    if (measured + 0.01 < required) {
      results.push({
        selector: selector(element),
        text: text.slice(0, 60),
        ratio: measured.toFixed(2),
        required,
      });
    }
  }
  return { results, scanned: nodes.length, withText, skippedBackground };
}

// --------------------------------------------------------------- Refract

const { render } = await import(
  pathToFileURL(path.join(refractCache, "packages/core/dist/index.js"))
);

for (const pageDef of pages.pages) {
  const url = `${baseUrl}/?${pageDef.query}`;
  const shots = await render({
    url,
    viewports: pages.viewports,
    out: path.join(root, artifacts, pageDef.id),
    freeze: true,
    concurrency: 3,
    waitForFunction:
      "document.querySelector('[data-app-status=\"ready\"]') !== null",
    networkIdleMs: 5_000,
  });
  for (const shot of shots) {
    for (const finding of shot.findings) {
      const rule = {
        horizontal_overflow: "horizontal-overflow",
        element_clipped: "element-clipped",
        text_overflow: "text-overflow",
        viewport_meta_missing: "viewport-meta",
      }[finding.type];
      if (!rule) {
        continue;
      }
      const selector = finding.selector ?? "document";
      emit(
        rule,
        `${pageDef.id}|${shot.preset}|${selector}`,
        `${pageDef.id} ${shot.preset} ${finding.type} ${finding.detail ?? ""} ${selector}`,
      );
    }
  }
}

// -------------------------------------------------------------- baseline

let baseline = { version: 1, entries: {} };
try {
  baseline = JSON.parse(await read(baselinePath));
} catch {
  baseline = { version: 1, entries: {} };
}

const locked = [];
const advisory = [];
for (const [key, finding] of [...findings.entries()].sort()) {
  if (ruleStatus(finding.rule) === "locked") {
    locked.push([key, finding]);
  } else {
    advisory.push([key, finding]);
  }
}

if (updateBaseline) {
  const entries = {};
  for (const [key, finding] of locked) {
    const previous = baseline.entries?.[key] ?? {};
    entries[key] = {
      count: finding.count,
      reason: previous.reason ?? "Initial design baseline",
      issue: previous.issue ?? baselineIssue,
      expires: previous.expires ?? null,
    };
  }
  await fs.mkdir(path.join(root, "design"), { recursive: true });
  await fs.writeFile(
    path.join(root, baselinePath),
    `${JSON.stringify(
      {
        version: 1,
        generatedAt: new Date().toISOString(),
        note: "Reviewed debt. Each entry needs a reason and an issue or an expiry.",
        entries,
      },
      null,
      2,
    )}\n`,
  );
  console.log(
    `design: baseline updated with ${Object.keys(entries).length} locked findings`,
  );
  process.exit(0);
}

const violations = [];
for (const [key, finding] of locked) {
  const entry = baseline.entries?.[key];
  if (!entry) {
    violations.push({
      key,
      finding,
      reason: "new finding",
    });
    continue;
  }
  if (entry.count !== undefined && finding.count > entry.count) {
    violations.push({
      key,
      finding,
      reason: `count ${finding.count} > baseline ${entry.count}`,
    });
  }
  const hasIssue = Boolean(entry.issue) && entry.issue !== "UNASSIGNED";
  if (!entry.reason || (!hasIssue && !entry.expires)) {
    violations.push({
      key,
      finding,
      reason: "baseline entry needs a reason and an issue or expiry",
    });
  }
  if (entry.expires && entry.expires < today) {
    violations.push({
      key,
      finding,
      reason: `baseline entry expired ${entry.expires}`,
    });
  }
}

const advisoryTotal = advisory.reduce((sum, [, finding]) => sum + finding.count, 0);
if (advisoryTotal > 0) {
  console.log(`design: ${advisoryTotal} advisory findings (not blocking)`);
  for (const [rule, count] of Object.entries(
    advisory.reduce((accumulator, [, finding]) => {
      accumulator[finding.rule] = (accumulator[finding.rule] ?? 0) + finding.count;
      return accumulator;
    }, {}),
  ).sort(([left], [right]) => left.localeCompare(right))) {
    console.log(`  advisory ${rule}: ${count}`);
  }
}

if (violations.length === 0) {
  console.log(
    `design: pass. ${locked.length} locked findings are covered by the baseline.`,
  );
  process.exit(0);
}

console.error(`design: fail. ${violations.length} locked design findings need action.`);
for (const violation of violations.slice(0, 60)) {
  console.error(`  [${violation.finding.rule}] ${violation.reason}`);
  console.error(`    ${violation.finding.sample}`);
  if (violation.finding.count > 1) {
    console.error(`    count ${violation.finding.count}`);
  }
}
if (violations.length > 60) {
  console.error(`  ... and ${violations.length - 60} more`);
}
console.error(
  "design: fix the finding, or run scripts/design-check.sh --update-baseline and review the diff. Baseline entries need a reason and an issue or expiry.",
);
process.exit(1);
