import { readFileSync, statSync } from "node:fs";
import { expect, test, bridgeToken, control, waitAssignment, waitEvent, type Bridge } from "./support";

const ACTIVE = ["mira", "jonas", "selma", "theo", "ines"];

test("the host runs a classic event from setup to the reveal ZIP", async ({
  page,
  bridge,
}) => {
  const token = await bridgeToken(bridge);
  await page.goto(bridge.url);

  await expect(
    page.getByRole("heading", { name: "Set up Monsquaz Swap" }),
  ).toBeVisible();

  // Connect the bot through the real setup dialog and the simulator.
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Add bot token" }).click();
  await expect(
    page.getByRole("heading", { name: "Connect the Discord bot" }),
  ).toBeVisible();
  await page.locator("#bot-token").fill(token);
  await page.getByRole("button", { name: "Validate token" }).click();
  await expect(page.getByText("Monsquaz Test Bot")).toBeVisible();
  await expect(page.locator("#bot-guild option")).toHaveCount(1);
  await expect(page.locator("#bot-guild option")).toContainText(
    "Monsquaz Swap Test Guild",
  );
  await page
    .locator("#bot-guild")
    .selectOption({ label: "Monsquaz Swap Test Guild" });
  await expect(page.locator("#bot-announcement option")).toContainText([
    "announcements",
    "reveal",
    "general",
  ]);
  await page.locator("#bot-announcement").selectOption("900000000000000011");
  await page.locator("#bot-reveal").selectOption("900000000000000012");
  await page.getByRole("button", { name: "Save connection" }).click();
  await expect(page.getByRole("button", { name: "Done" })).toBeVisible();
  await page.locator("#bot-test-user").fill("910000000000000001");
  await page.getByRole("button", { name: "Send test", exact: true }).click();
  await expect(page.getByText("The test message was sent")).toBeVisible();
  await page.getByRole("button", { name: "Done" }).click();

  // Create the event in the setup wizard.
  await page.getByRole("button", { name: "Event setup" }).click();
  await page.locator("#event-name").fill("Browser Classic");
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Create event" }).click();
  await expect(
    page.getByRole("heading", { name: "Browser Classic", exact: true }),
  ).toBeVisible();

  // Upload the pack and open the seed round.
  await page.getByRole("button", { name: /Upload host pack/ }).click();
  await expect(page.getByText(/pack\.zip/)).toBeVisible();
  await page.getByRole("button", { name: "Open Round 1" }).click();
  await expect(page.getByText(/Round 1 · Seed round/)).toBeVisible();

  // Five people send a seed. One person joins without a seed.
  for (const user of ACTIVE) {
    await control(bridge, "join", user);
    await control(bridge, "seed", user);
  }
  await control(bridge, "join", "rune");
  await waitParticipant(bridge, 6, 5);
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Browser Classic", exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: "../target/e2e/screens/classic-seed-round-1280.png",
    fullPage: true,
  });

  // Lock the roster.
  await page.getByRole("button", { name: "Lock roster", exact: true }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Lock roster", exact: true })
    .click();
  await expect(page.locator(".toast")).toContainText("Roster locked");

  // Generate the schedule and lock it.
  await page.getByRole("button", { name: "Open schedule planner" }).click();
  await expect(
    page.getByRole("heading", { name: "Schedule planner" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Generate plan" }).click();
  await expect(
    page.getByRole("heading", { name: "Assignment matrix" }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Lock schedule", exact: true })
    .first()
    .click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Lock schedule" })
    .click();
  await page.getByRole("button", { name: "Mission control" }).click();
  await expect(
    page.getByRole("button", { name: "Open Round 2" }),
  ).toBeVisible();

  // Round 2: five people work, one skips, and one fill-in rescues the track.
  await page.getByRole("button", { name: "Open Round 2" }).click();
  await expect(page.locator(".toast")).toContainText("Round 2 opened");
  let skippedTrack = "";
  for (const user of ACTIVE) {
    await control(bridge, "claim", user);
    const claimed = await waitAssignment(bridge, user, "Claimed");
    if (user === "theo") {
      await control(bridge, "skip", user);
      await waitAssignment(bridge, user, "SkippedAfterClaim");
      skippedTrack = claimed.trackCode ?? "";
      expect(skippedTrack).not.toBe("");
    } else {
      await control(bridge, "submit", user);
      await waitAssignment(bridge, user, "Submitted");
    }
  }

  // The host assigns the fill-in through the mission control dialog.
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Browser Classic", exact: true }),
  ).toBeVisible();
  await page.screenshot({
    path: "../target/e2e/screens/classic-round-progress-1280.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Fill-in" }).click();
  await page.locator("#fill-in-track").fill(skippedTrack);
  await page.locator("#fill-in-round").fill("2");
  await page.getByRole("button", { name: "Find people" }).click();
  await page.locator("#fill-in-person").selectOption("910000000000000007");
  await page.getByRole("button", { name: "Assign fill-in" }).click();
  await expect(
    page.getByText("Fill-in assigned. The offer is queued."),
  ).toBeVisible();
  await control(bridge, "claim", "rune");
  await waitAssignment(bridge, "rune", "Claimed");
  await control(bridge, "submit", "rune");
  await waitAssignment(bridge, "rune", "Submitted");

  // Move the open deadline through the dialog.
  await page.reload();
  await page.getByRole("button", { name: "Move deadline" }).click();
  await page.locator("#move-round-reason").fill("A server outage delayed edits");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Move deadline" })
    .click();
  await expect(page.locator(".toast")).toContainText("deadline moved");

  // Rounds 3, 4, and 5: everyone works.
  for (const round of [3, 4, 5]) {
    await page.reload();
    await page.getByRole("button", { name: `Open Round ${round}` }).click();
    await expect(page.locator(".toast")).toContainText(
      new RegExp(`Round ${round} opened`),
    );
    for (const user of ACTIVE) {
      await control(bridge, "claim", user);
      await waitAssignment(bridge, user, "Claimed");
      await control(bridge, "submit", user);
      await waitAssignment(bridge, user, "Submitted");
    }
  }

  // Reveal, then export the ZIP.
  await page.reload();
  await page.getByRole("button", { name: "Reveal event" }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Reveal event" })
    .click();
  await expect(page.getByRole("heading", { name: "Reveal show" })).toBeVisible();
  await expect(page.getByText(/Browser Classic/).first()).toBeVisible();
  await page.screenshot({
    path: "../target/e2e/screens/classic-reveal-1280.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Export ZIP" }).click();
  await expect(page.locator(".toast")).toContainText("Reveal ZIP saved");
  const zip = readFileSync(bridge.exportPath);
  expect(zip.subarray(0, 2).toString()).toBe("PK");
  expect(statSync(bridge.exportPath).size).toBeGreaterThan(100);

  // The minimum width still fits the reveal screen.
  await page.setViewportSize({ width: 1024, height: 700 });
  await page.reload();
  await page.getByRole("button", { name: "Reveal show" }).click();
  await expect(page.getByRole("heading", { name: "Reveal show" })).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
  await page.screenshot({
    path: "../target/e2e/screens/classic-reveal-1024.png",
    fullPage: true,
  });
});

async function waitParticipant(
  bridge: Bridge,
  people: number,
  tracks: number,
): Promise<void> {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    const state = (await fetch(`${bridge.url}/api/control/state`).then((r) =>
      r.json(),
    )) as { value: { participants: number; tracks: number } };
    if (
      state.value.participants === people &&
      state.value.tracks === tracks
    ) {
      return;
    }
    await new Promise((done) => setTimeout(done, 40));
  }
  throw new Error("the seed roster did not become ready");
}
