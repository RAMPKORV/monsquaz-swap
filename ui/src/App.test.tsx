import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import App from "./App";

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn().mockResolvedValue(true),
  requestPermission: vi.fn().mockResolvedValue("granted"),
  sendNotification: vi.fn(),
}));

function setUrl(search: string) {
  window.history.pushState({}, "", `/${search}`);
}

beforeEach(() => {
  setUrl("?demo=1");
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

describe("App", () => {
  it("shows the mission control next action", async () => {
    render(<App />);
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Night Shift Vol. 2",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", {
        level: 2,
        name: "Three decisions need a host call",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /review decisions/i }),
    ).toBeInTheDocument();
  });

  it("shows the first-run state", async () => {
    setUrl("?demo=1&state=empty");
    render(<App />);
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: /give your server a strange and beautiful album/i,
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /start setup/i }),
    ).toBeInTheDocument();
  });

  it("filters the audit log", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Audit log" }));
    expect(
      await screen.findByRole("heading", {
        level: 1,
        name: "Every decision, in order",
      }),
    ).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Search the audit log"), {
      target: { value: "S7F3A" },
    });
    expect(screen.getByText("2 of 14 entries")).toBeInTheDocument();
  });

  it("confirms destructive deletion with a typed word", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", {
      level: 1,
      name: "The quiet parts of the operation",
    });
    fireEvent.click(screen.getByRole("button", { name: "Delete data" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    const confirm = screen.getByRole("button", { name: "Delete local data" });
    expect(confirm).toBeDisabled();
    const input = screen.getByLabelText("Type delete to confirm");
    input.focus();
    fireEvent.change(input, { target: { value: "delete" } });
    expect(input).toHaveFocus();
    expect(confirm).toBeEnabled();
  });

  it("switches between dark and light themes", async () => {
    const { container } = render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    const app = container.querySelector(".app");
    expect(app).toHaveAttribute("data-theme", "dark");
    fireEvent.click(
      screen.getByRole("button", { name: "Switch to light theme" }),
    );
    expect(app).toHaveAttribute("data-theme", "light");
    fireEvent.click(
      screen.getByRole("button", { name: "Switch to dark theme" }),
    );
    expect(app).toHaveAttribute("data-theme", "dark");
  });

  it("selects the theme from the settings radio group", async () => {
    const { container } = render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", {
      level: 1,
      name: "The quiet parts of the operation",
    });
    fireEvent.click(screen.getByRole("radio", { name: /^Light/ }));
    expect(container.querySelector(".app")).toHaveAttribute(
      "data-theme",
      "light",
    );
  });

  it("moves focus to the page heading after navigation", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Audit log" }));
    const heading = await screen.findByRole("heading", {
      level: 1,
      name: "Every decision, in order",
    });
    expect(heading).toHaveFocus();
    expect(document.title).toBe("Audit log · Monsquaz Swap");
  });

  it("saves settings and reports the result", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", {
      level: 1,
      name: "The quiet parts of the operation",
    });
    const retention = screen.getByLabelText("Keep files for");
    fireEvent.change(retention, { target: { value: "30" } });
    fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
    expect(
      await screen.findByText("Settings saved on this computer."),
    ).toBeInTheDocument();
  });

  it("offers a notification test in settings", async () => {
    render(<App />);
    await screen.findByRole("heading", { level: 1, name: "Night Shift Vol. 2" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    await screen.findByRole("heading", {
      level: 1,
      name: "The quiet parts of the operation",
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Send test notification" }),
    );
    expect(
      await screen.findByText(
        "Preview: native notifications are not sent in this build.",
      ),
    ).toBeInTheDocument();
  });

  it("shows a recoverable error state", async () => {
    setUrl("?demo=1&state=error");
    render(<App />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("MSW-UI-001");
    expect(
      screen.getByRole("button", { name: "Retry" }),
    ).toBeInTheDocument();
  });
});
