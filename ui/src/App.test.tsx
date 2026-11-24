import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({
    status: "ok",
    appVersion: "0.1.0",
    coreVersion: "0.1.0",
  }),
}));

describe("App", () => {
  it("renders the product name", () => {
    render(<App />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(
      "Monsquaz",
    );
  });

  it("reports core health when asked", async () => {
    render(<App />);
    fireEvent.click(screen.getByRole("button", { name: /check core/i }));
    expect(await screen.findByTestId("health")).toHaveTextContent("ok");
  });
});
