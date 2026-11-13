import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Settings } from "../types";
import { I18nProvider } from "../lib/i18n";
import { openSponsorPage } from "../lib/backend";
import { AboutDialog, SettingsDialog } from "./Dialogs";

vi.mock("../lib/backend", async (original) => ({
  ...(await original<typeof import("../lib/backend")>()),
  openSponsorPage: vi.fn()
}));

const settings: Settings = {
  language: "system",
  theme: "system",
  colorScheme: "system",
  byteUnitScale: "binary",
  contrast: 72,
  showSidebar: true,
  keepHistory: true,
  scanOptions: {
    crossFilesystems: false,
    includeRemoteMounts: false,
    includeRemovable: true,
    showSmallFiles: false,
    exclusions: ["/proc", "/sys", "/dev", "/run"]
  }
};

describe("SettingsDialog localization", () => {
  it("offers System and every supported language and saves the selection", () => {
    const onSave = vi.fn();
    render(
      <I18nProvider locale="en">
        <SettingsDialog
          open
          onOpenChange={() => undefined}
          settings={settings}
          systemLocale="en"
          onSave={onSave}
          onClearHistory={() => undefined}
        />
      </I18nProvider>
    );

    const language = screen.getByLabelText("Language");
    expect(language.querySelectorAll("option")).toHaveLength(12);
    expect(language.querySelector('option[value="system"]')).toHaveTextContent(
      /^System — /
    );

    fireEvent.change(language, { target: { value: "tr" } });
    fireEvent.click(screen.getByRole("button", { name: "Save settings" }));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ language: "tr" })
    );
  });
});

describe("AboutDialog", () => {
  it("offers a quiet sponsor link that only acts on click", () => {
    render(
      <I18nProvider locale="en">
        <AboutDialog open onOpenChange={() => undefined} />
      </I18nProvider>
    );
    expect(openSponsorPage).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "Support development on GitHub Sponsors" })
    );
    expect(openSponsorPage).toHaveBeenCalledTimes(1);
  });
});
