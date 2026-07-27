import { describe, expect, it } from "vitest";
import type { VolumeInfo } from "../types";
import { findVolumeForTarget, getVolumeCapacity } from "./volume";

const volumes: VolumeInfo[] = [
  {
    name: "System",
    mountPoint: "/",
    fileSystem: "btrfs",
    totalBytes: 1000,
    availableBytes: 250,
    removable: false,
    remote: false
  },
  {
    name: "Home",
    mountPoint: "/home",
    fileSystem: "btrfs",
    totalBytes: 1000,
    availableBytes: 250,
    removable: false,
    remote: false
  },
  {
    name: "Archive",
    mountPoint: "/mnt/archive",
    fileSystem: "ext4",
    totalBytes: 2000,
    availableBytes: 1200,
    removable: false,
    remote: false
  }
];

describe("findVolumeForTarget", () => {
  it("chooses the longest containing mount point", () => {
    expect(findVolumeForTarget(volumes, "/home/alex")?.name).toBe("Home");
    expect(findVolumeForTarget(volumes, "/var/lib")?.name).toBe("System");
    expect(
      findVolumeForTarget(volumes, "file:///mnt/archive/projects")?.name
    ).toBe("Archive");
  });

  it("does not associate remote targets with a local volume", () => {
    expect(findVolumeForTarget(volumes, "sftp://host/home")).toBeNull();
  });
});

describe("getVolumeCapacity", () => {
  it("calculates used, free, total, and used percentage", () => {
    expect(getVolumeCapacity(volumes[0])).toEqual({
      usedBytes: 750,
      freeBytes: 250,
      totalBytes: 1000,
      usedRatio: 0.75,
      usedPercent: 75
    });
  });

  it("clamps invalid free-space values to the volume bounds", () => {
    expect(
      getVolumeCapacity({ ...volumes[0], availableBytes: 1200 }).usedBytes
    ).toBe(0);
    expect(
      getVolumeCapacity({ ...volumes[0], availableBytes: -20 }).freeBytes
    ).toBe(0);
  });
});
