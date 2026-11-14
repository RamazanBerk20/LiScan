import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { I18nProvider } from "../lib/i18n";
import { WindowControls } from "./WindowControls";

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("WindowControls", () => {
  it("renders nothing outside the desktop app", () => {
    const { container } = render(<WindowControls />);
    expect(container).toBeEmptyDOMElement();
  });

  it("labels the buttons, shows Restore when maximised and closes the window", async () => {
    const calls: string[] = [];
    mockWindows("main");
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === "plugin:window|is_maximized") return true;
    });
    render(
      <I18nProvider locale="tr">
        <WindowControls />
      </I18nProvider>
    );
    expect(screen.getByRole("button", { name: "Küçült" })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Geri yükle" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Kapat" }));
    await Promise.resolve();
    expect(calls).toContain("plugin:window|close");
  });
});
