import { HardDrive } from "@phosphor-icons/react";
import type { ByteUnitScale, VolumeInfo } from "../types";
import { formatBytes, formatPercentage } from "../lib/format";
import { useI18n } from "../lib/i18n";
import { getVolumeCapacity } from "../lib/volume";

export function CapacityStrip({
  volume,
  byteUnitScale
}: {
  volume: VolumeInfo;
  byteUnitScale: ByteUnitScale;
}) {
  const { locale, t } = useI18n();
  const capacity = getVolumeCapacity(volume);
  const percentage = formatPercentage(capacity.usedPercent, locale);
  const volumeName =
    volume.mountPoint === "/"
      ? t("systemFilesystem")
      : volume.mountPoint;

  return (
    <section
      className="capacity-strip"
      aria-label={t("storageCapacityFor", { volume: volumeName })}
    >
      <div className="capacity-strip__volume">
        <span className="capacity-strip__icon" aria-hidden="true">
          <HardDrive size={19} weight="duotone" />
        </span>
        <span>
          <small dir="auto">
            {t("mountedAt", {
              filesystem: volume.fileSystem || t("filesystem"),
              mount: volume.mountPoint
            })}
          </small>
          <strong title={volumeName}>{volumeName}</strong>
        </span>
      </div>

      <dl className="capacity-strip__stats">
        <div>
          <dt>{t("used")}</dt>
          <dd>{formatBytes(capacity.usedBytes, byteUnitScale, locale)}</dd>
        </div>
        <div>
          <dt>{t("free")}</dt>
          <dd>{formatBytes(capacity.freeBytes, byteUnitScale, locale)}</dd>
        </div>
        <div>
          <dt>{t("total")}</dt>
          <dd>{formatBytes(capacity.totalBytes, byteUnitScale, locale)}</dd>
        </div>
      </dl>

      <div className="capacity-strip__usage">
        <span>
          <small>{t("usedTotal")}</small>
          <strong>{percentage}</strong>
        </span>
        <span
          className="meter"
          role="progressbar"
          aria-label={t("storageUsedAria", { percentage })}
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
