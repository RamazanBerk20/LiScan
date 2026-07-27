import { HardDrive } from "@phosphor-icons/react";
import type { ByteUnitScale, VolumeInfo } from "../types";
import { formatBytes, formatPercentage } from "../lib/format";
import { getVolumeCapacity } from "../lib/volume";

export function CapacityStrip({
  volume,
  byteUnitScale
}: {
  volume: VolumeInfo;
  byteUnitScale: ByteUnitScale;
}) {
  const capacity = getVolumeCapacity(volume);
  const percentage = formatPercentage(capacity.usedPercent);
  const volumeName =
    volume.mountPoint === "/"
      ? "System filesystem"
      : volume.mountPoint;

  return (
    <section
      className="capacity-strip"
      aria-label={`Storage capacity for ${volumeName}`}
    >
      <div className="capacity-strip__volume">
        <span className="capacity-strip__icon" aria-hidden="true">
          <HardDrive size={19} weight="duotone" />
        </span>
        <span>
          <small>
            {volume.fileSystem || "filesystem"} · mounted at {volume.mountPoint}
          </small>
          <strong title={volumeName}>{volumeName}</strong>
        </span>
      </div>

      <dl className="capacity-strip__stats">
        <div>
          <dt>Used</dt>
          <dd>{formatBytes(capacity.usedBytes, byteUnitScale)}</dd>
        </div>
        <div>
          <dt>Free</dt>
          <dd>{formatBytes(capacity.freeBytes, byteUnitScale)}</dd>
        </div>
        <div>
          <dt>Total</dt>
          <dd>{formatBytes(capacity.totalBytes, byteUnitScale)}</dd>
        </div>
      </dl>

      <div className="capacity-strip__usage">
        <span>
          <small>Used / total</small>
          <strong>{percentage}</strong>
        </span>
        <span
          className="meter"
          role="progressbar"
          aria-label={`${percentage} of storage used`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(capacity.usedPercent)}
        >
          <span style={{ width: `${capacity.usedPercent}%` }} />
        </span>
      </div>
    </section>
  );
}
