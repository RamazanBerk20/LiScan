import type { ByteUnitScale } from "../types";

const BYTE_UNITS: Record<
  ByteUnitScale,
  { base: number; labels: readonly string[] }
> = {
  binary: {
    base: 1024,
    labels: ["B", "KiB", "MiB", "GiB", "TiB", "PiB"]
  },
  decimal: {
    base: 1000,
    labels: ["B", "kB", "MB", "GB", "TB", "PB"]
  }
};

export function formatBytes(
  value: number,
  scale: ByteUnitScale = "binary"
): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const { base, labels } = BYTE_UNITS[scale];
  const index = Math.min(
    Math.floor(Math.log(value) / Math.log(base)),
    labels.length - 1
  );
  const scaled = value / base ** index;
  const digits = scaled >= 100 || index === 0 ? 0 : scaled >= 10 ? 1 : 2;
  return `${scaled.toFixed(digits)} ${labels[index]}`;
}

export function formatCount(value: number): string {
  return new Intl.NumberFormat().format(value);
}

export function formatPercentage(value: number): string {
  const safeValue = Number.isFinite(value)
    ? Math.min(100, Math.max(0, value))
    : 0;
  return `${new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1
  }).format(safeValue)}%`;
}

export function leafName(path: string): string {
  const clean = path.replace(/\/+$/, "");
  if (!clean) return "/";
  const parts = clean.split("/");
  return parts.at(-1) || "/";
}

export function parentPath(path: string): string | null {
  if (path === "/") return null;
  const clean = path.replace(/\/+$/, "");
  const index = clean.lastIndexOf("/");
  if (index <= 0) return "/";
  return clean.slice(0, index);
}
