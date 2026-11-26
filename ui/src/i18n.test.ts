import { describe, expect, it } from "vitest";
import {
  format,
  formatDateTime,
  joinInstructions,
  messages,
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
    const text = joinInstructions("Europe/Stockholm", 4, ["it", "xm"], 20);
    expect(text).toContain(
      format(messages.join.command, {
        timezone: "Europe/Stockholm",
        handoffs: 4,
      }),
    );
    expect(text).toContain(".it, .xm");
    expect(text).toContain("20 MiB");
  });
});
