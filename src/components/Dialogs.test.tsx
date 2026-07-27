import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Settings } from "../types";
import { I18nProvider } from "../lib/i18n";
import { SettingsDialog } from "./Dialogs";

const settings: Settings = {
  language: "system",
  theme: "system",
  colorScheme: "system",
  byteUnitScale: "binary",
  contrast: 72,
  showSidebar: true,
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
