import { describe, expect, it } from "vitest";
import {
  format,
  formatDateTime,
  joinInstructions,
  plural,
} from "./i18n";

describe("i18n", () => {
  it("formats named values", () => {
    expect(format("Hello {name}, {count} files", { name: "Noor", count: 3 })).toBe(
      "Hello Noor, 3 files",
    );
    expect(format("Keep {missing}", {})).toBe("Keep {missing}");
  });

  it("uses the plural rule", () => {
    expect(plural(1, "comment", "comments")).toBe("comment");
    expect(plural(0, "comment", "comments")).toBe("comments");
    expect(plural(2, "comment", "comments")).toBe("comments");
  });

  it("formats dates in an explicit timezone", () => {
    const value = formatDateTime(1_735_689_600, "Europe/Stockholm", {
      dateStyle: "medium",
      timeStyle: "short",
    });
    expect(value).toContain("2025");
  });

  it("builds the copyable join instructions", () => {
    const text = joinInstructions("Europe/Stockholm", ["it", "xm"], 20);
    expect(text).toContain("1. In the server, run /swap join.");
    expect(text).toContain("2. Choose your timezone");
    expect(text).toContain("3. Send one seed file to the bot");
    expect(text).toContain(".it, .xm");
    expect(text).toContain("20 MiB");
  });
});
