import type { ByteUnitScale, DiffRow } from "../types";
import { formatBytes } from "./format";

/** Rows that grew (largest first) and rows that shrank (largest first). */
export function splitDiff(rows: DiffRow[]): { grew: DiffRow[]; shrank: DiffRow[] } {
  const grew = rows.filter((row) => row.delta > 0);
  const shrank = rows.filter((row) => row.delta < 0);
  const byMagnitude = (left: DiffRow, right: DiffRow) =>
    Math.abs(right.delta) - Math.abs(left.delta);
  return { grew: grew.sort(byMagnitude), shrank: shrank.sort(byMagnitude) };
}

/** Signed size so the direction never depends on colour alone. */
export function formatDelta(
  delta: number,
  scale: ByteUnitScale,
  locale: string
): string {
  if (delta === 0) return formatBytes(0, scale, locale);
  return `${delta > 0 ? "+" : "−"}${formatBytes(Math.abs(delta), scale, locale)}`;
}

/** Folder path -> change, plus the largest change (the colour scale's top). */
export function deltaIndex(rows: DiffRow[]): {
  deltas: Map<string, number>;
  max: number;
} {
  const deltas = new Map<string, number>();
  let max = 0;
  for (const row of rows) {
    deltas.set(row.path, row.delta);
    max = Math.max(max, Math.abs(row.delta));
  }
  return { deltas, max };
}

/** 0 (barely changed) to 1 (the largest change); square-rooted so small changes stay visible. */
export function intensity(delta: number, max: number): number {
  return max > 0 ? Math.sqrt(Math.min(1, Math.abs(delta) / max)) : 0;
}

/** True when `path` is one of the matched folders or lies inside one. */
export function insideAny(path: string, roots: readonly string[]): boolean {
  return roots.some((root) => path === root || path.startsWith(`${root}/`));
}
