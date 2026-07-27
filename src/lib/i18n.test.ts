import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { render } from "@testing-library/react";
import {
  I18nProvider,
  languageOptions,
  resolveLocale,
  translate,
  type MessageKey
} from "./i18n";

describe("resolveLocale", () => {
  it("uses the first supported system language", () => {
    expect(resolveLocale("system", ["pt-BR", "tr-TR", "en-US"])).toBe("tr");
    expect(resolveLocale("system", ["zh-CN"])).toBe("zh");
  });

  it("falls back to English for an unsupported system language", () => {
    expect(resolveLocale("system", ["pt-BR"])).toBe("en");
  });

  it("keeps an explicit language preference", () => {
    expect(resolveLocale("ja", ["tr-TR"])).toBe("ja");
  });
});

describe("translations", () => {
  it("offers every supported language", () => {
    expect(languageOptions.map(({ value }) => value)).toEqual([
      "en",
      "tr",
      "es",
      "it",
      "fr",
      "de",
      "ru",
      "ar",
      "zh",
      "ja",
      "ko"
    ]);
  });

  it("interpolates values", () => {
    expect(translate("tr", "filesCount", { count: "1.234" })).toBe(
      "1.234 dosya"
    );
  });

  it("has localized values for representative UI surfaces", () => {
    const keys: MessageKey[] = [
      "overviewTitle",
      "settingsTitle",
      "adminTitle",
      "coverageTitle",
      "scanFailureTitle"
    ];
    for (const locale of languageOptions.map(({ value }) => value).slice(1)) {
      for (const key of keys) {
        expect(translate(locale, key)).not.toBe(translate("en", key));
      }
    }
  });
});

describe("I18nProvider", () => {
  it("updates the document language and direction", () => {
    const { rerender } = render(
      createElement(I18nProvider, {
        locale: "ar",
        children: createElement("span")
      })
    );
    expect(document.documentElement.lang).toBe("ar");
    expect(document.documentElement.dir).toBe("rtl");

    rerender(
      createElement(I18nProvider, {
        locale: "zh",
        children: createElement("span")
      })
    );
    expect(document.documentElement.lang).toBe("zh-CN");
    expect(document.documentElement.dir).toBe("ltr");
  });
});
