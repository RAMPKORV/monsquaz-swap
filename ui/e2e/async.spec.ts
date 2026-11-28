import { expect, test, bridgeToken, control, waitEvent, waitAssignment } from "./support";

const PEOPLE = ["mira", "jonas", "selma", "theo"];

test("the host runs an async queue from intake to the reveal", async ({
  page,
  bridge,
}) => {
  const token = await bridgeToken(bridge);
  await page.goto(bridge.url);

  // Connect the bot.
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Add bot token" }).click();
  await page.locator("#bot-token").fill(token);
  await page.getByRole("button", { name: "Validate token" }).click();
  await expect(page.getByText("Monsquaz Test Bot")).toBeVisible();
  await page
    .locator("#bot-guild")
    .selectOption({ label: "Monsquaz Swap Test Guild" });
  await expect(page.locator("#bot-announcement option")).toHaveCount(3);
  await page.locator("#bot-announcement").selectOption("900000000000000011");
  await page.locator("#bot-reveal").selectOption("900000000000000012");
  await page.getByRole("button", { name: "Save connection" }).click();
  await expect(page.getByRole("button", { name: "Done" })).toBeVisible();
  await page.getByRole("button", { name: "Done" }).click();

  // Create the async event.
  await page.getByRole("button", { name: "Event setup" }).click();
  await page.locator("#event-name").fill("Browser Async");
  await page.getByRole("radio", { name: /Async/ }).click();
  await page.locator("#async-songs").fill("4");
  await page.locator("#async-claims").fill("2");
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Continue" }).click();
  await page.getByRole("button", { name: "Create event" }).click();
  await expect(
    page.getByRole("heading", { name: "Browser Async", exact: true }),
  ).toBeVisible();

  // Open seed intake and collect four seeds.
  await page.getByRole("button", { name: /Upload host pack/ }).click();
  await expect(page.getByText(/pack\.zip/)).toBeVisible();
  await page.getByRole("button", { name: "Open seed intake" }).click();
  for (const user of PEOPLE) {
    await control(bridge, "join", user);
    await control(bridge, "seed", user);
  }
  await waitRoster(bridge, 4, 4);
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Browser Async", exact: true }),
  ).toBeVisible();

  // Lock the roster and start the queue.
  await page.getByRole("button", { name: "Lock roster", exact: true }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Lock roster", exact: true })
    .click();
  await expect(page.locator(".toast")).toContainText("Roster locked");
  await page.getByRole("button", { name: "Start async queue" }).click();
  await waitEvent(bridge, "Active");
  await expect(
    page.getByRole("heading", { name: "Async queue" }),
  ).toBeVisible();

  // Send one wave and let every person claim and submit.
  await page.getByRole("button", { name: "Send offers now" }).click();
  await expect(page.locator(".toast")).toContainText("Wave dispatched");
  for (const user of PEOPLE) {
    await control(bridge, "claim", user);
    await waitAssignment(bridge, user, "Claimed");
    await control(bridge, "submit", user);
    await waitAssignment(bridge, user, "Submitted");
  }

  // The queue overview shows real coverage.
  await page.reload();
  await expect(page.getByRole("heading", { name: "Async queue" })).toBeVisible();
  await expect(page.getByText(/4 of 8/).first()).toBeVisible();
  await expect(page.getByText(/pairs not exposed/).first()).toBeVisible();
  await page.screenshot({
    path: "../target/e2e/screens/async-queue-1280.png",
    fullPage: true,
  });

  // Publish the reveal.
  await page.getByRole("button", { name: "Reveal show" }).click();
  await page.getByRole("button", { name: "Publish reveal" }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Publish reveal" })
    .click();
  await expect(page.getByText("Published", { exact: true })).toBeVisible();
  await expect(page.getByText(/Browser Async/).first()).toBeVisible();

  // The minimum width still fits the reveal screen.
  await page.setViewportSize({ width: 1024, height: 700 });
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Async queue" }),
  ).toBeVisible();
  await page.screenshot({
    path: "../target/e2e/screens/async-queue-1024.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Reveal show" }).click();
  await expect(
    page.getByRole("heading", { name: "Reveal show" }),
  ).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow).toBeLessThanOrEqual(1);
  await page.screenshot({
    path: "../target/e2e/screens/async-reveal-1024.png",
    fullPage: true,
  });
});

async function waitRoster(
  bridge: { url: string },
  people: number,
  tracks: number,
): Promise<void> {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    const response = await fetch(`${bridge.url}/api/control/state`);
    const body = (await response.json()) as {
      value: { participants: number; tracks: number };
    };
    if (body.value.participants === people && body.value.tracks === tracks) {
      return;
    }
    await new Promise((done) => setTimeout(done, 40));
  }
  throw new Error("the async roster did not become ready");
}
