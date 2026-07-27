import type { VolumeInfo } from "../types";

export interface VolumeCapacity {
  usedBytes: number;
  freeBytes: number;
  totalBytes: number;
  usedRatio: number;
  usedPercent: number;
}

function normalizeMountPoint(path: string): string {
  if (path === "/") return path;
  return path.replace(/\/+$/, "");
}

function localPathFromTarget(target: string): string | null {
  if (!target.includes("://")) return target;
  if (!target.startsWith("file://")) return null;
  try {
    return decodeURIComponent(new URL(target).pathname);
  } catch {
    return null;
  }
}

export function findVolumeForTarget(
  volumes: VolumeInfo[],
  target: string
): VolumeInfo | null {
  const path = localPathFromTarget(target);
  if (!path?.startsWith("/")) return null;

  return (
    volumes
      .filter((volume) => {
        const mount = normalizeMountPoint(volume.mountPoint);
        return mount === "/" || path === mount || path.startsWith(`${mount}/`);
      })
      .sort(
        (left, right) =>
          normalizeMountPoint(right.mountPoint).length -
          normalizeMountPoint(left.mountPoint).length
      )[0] ?? null
  );
}

export function getVolumeCapacity(volume: VolumeInfo): VolumeCapacity {
  const totalBytes = Math.max(0, volume.totalBytes);
  const freeBytes = Math.min(
    totalBytes,
    Math.max(0, volume.availableBytes)
  );
  const usedBytes = Math.max(0, totalBytes - freeBytes);
  const usedRatio = totalBytes > 0 ? usedBytes / totalBytes : 0;
  return {
    usedBytes,
    freeBytes,
    totalBytes,
    usedRatio,
    usedPercent: usedRatio * 100
  };
}
