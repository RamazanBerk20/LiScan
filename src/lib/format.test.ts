import { describe, expect, it } from "vitest";
import {
  formatBytes,
  formatPercentage,
  leafName,
  parentPath
} from "./format";

describe("formatBytes", () => {
  it("uses IEC binary units with a 1024-byte scale", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1024)).toBe("1.00 KiB");
    expect(formatBytes(10 * 1024 ** 3)).toBe("10.0 GiB");
  });

  it("uses SI decimal units with a 1000-byte scale", () => {
    expect(formatBytes(1000, "decimal")).toBe("1.00 kB");
    expect(formatBytes(10_000_000_000, "decimal")).toBe("10.0 GB");
    expect(formatBytes(1024, "decimal")).toBe("1.02 kB");
  });

  it("handles invalid values safely", () => {
    expect(formatBytes(Number.NaN)).toBe("0 B");
    expect(formatBytes(-20)).toBe("0 B");
  });
});

describe("formatPercentage", () => {
  it("formats and clamps percentages", () => {
    expect(formatPercentage(67.25)).toBe("67.3%");
    expect(formatPercentage(-1)).toBe("0%");
    expect(formatPercentage(120)).toBe("100%");
  });
});

describe("path helpers", () => {
  it("handles roots and nested paths", () => {
    expect(leafName("/")).toBe("/");
    expect(leafName("/home/alex/")).toBe("alex");
    expect(parentPath("/")).toBeNull();
    expect(parentPath("/home/alex")).toBe("/home");
    expect(parentPath("/home")).toBe("/");
  });
});
