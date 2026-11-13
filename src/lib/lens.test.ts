import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import type { DiffRow } from "../types";
import { deltaIndex, formatDelta, insideAny, intensity, splitDiff } from "./lens";
import { translate, type MessageKey } from "./i18n";

const row = (path: string, before: number, after: number): DiffRow => ({
  path,
  before,
  after,
  delta: after - before,
  nodeId: null
});

describe("comparison helpers", () => {
  const rows = [row("/a", 10, 15), row("/b", 100, 20), row("/c", 5, 5), row("/d", 0, 90)];

  it("splits into grew and shrank, largest first, dropping unchanged", () => {
    const { grew, shrank } = splitDiff(rows);
    expect(grew.map((r) => r.path)).toEqual(["/d", "/a"]);
    expect(shrank.map((r) => r.path)).toEqual(["/b"]);
  });

  it("signs the delta with a plus or a real minus", () => {
    expect(formatDelta(2048, "binary", "en")).toBe("+2.00 KiB");
    expect(formatDelta(-2048, "binary", "en")).toBe("−2.00 KiB");
    expect(formatDelta(0, "binary", "en")).toBe("0 B");
  });

  it("scales intensity by the largest change", () => {
    const { deltas, max } = deltaIndex(rows);
    expect(max).toBe(90);
    expect(deltas.get("/b")).toBe(-80);
    expect(intensity(90, max)).toBe(1);
    expect(intensity(0, max)).toBe(0);
    expect(intensity(5, 0)).toBe(0);
    expect(intensity(-22.5, max)).toBeCloseTo(0.5);
  });

  it("matches a folder and what is inside it, not a name prefix", () => {
    expect(insideAny("/h/.cache", ["/h/.cache"])).toBe(true);
    expect(insideAny("/h/.cache/pip/x", ["/h/.cache"])).toBe(true);
    expect(insideAny("/h/.cache2", ["/h/.cache"])).toBe(false);
  });
});

describe("reclaim rule keys", () => {
  it("are all defined in every message table", () => {
    const source = readFileSync("src-tauri/src/reclaim.rs", "utf8");
    const keys = [...source.matchAll(/_key: "(\w+)"/g)].map((m) => m[1]);
    expect(keys.length).toBeGreaterThanOrEqual(34);
    for (const locale of ["en", "tr", "ar", "ja"] as const) {
      for (const key of keys) {
        expect(translate(locale, key as MessageKey)).not.toBe("undefined");
        expect(translate(locale, key as MessageKey)).toBeTruthy();
      }
    }
  });
});
