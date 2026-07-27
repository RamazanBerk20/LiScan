import {
  ArrowRight,
  Folder,
  HardDrive,
  House,
  LinkSimple,
  LockKey,
  ShieldCheck
} from "@phosphor-icons/react";
import type { ByteUnitScale, VolumeInfo } from "../types";
import { formatBytes, formatPercentage } from "../lib/format";
import { getVolumeCapacity } from "../lib/volume";
import { useI18n } from "../lib/i18n";
import { LiScanMark } from "./Brand";

interface OverviewProps {
  volumes: VolumeInfo[];
  volumesLoading: boolean;
  homePath: string;
  byteUnitScale: ByteUnitScale;
  onScan: (target: string) => void;
  onChooseFolder: () => void;
  onRemote: () => void;
  onAdmin: () => void;
  onSettings: () => void;
  onAbout: () => void;
}

export function Overview({
  volumes,
  volumesLoading,
  homePath,
  byteUnitScale,
  onScan,
  onChooseFolder,
  onRemote,
  onAdmin,
  onSettings,
  onAbout
}: OverviewProps) {
  const { locale, t } = useI18n();

  return (
    <div className="overview-shell">
      <header className="overview-nav">
        <LiScanMark />
        <div className="overview-nav__actions">
          <button className="button button--quiet" onClick={onAbout}>
            {t("about")}
          </button>
          <button className="button button--quiet" onClick={onSettings}>
            {t("settings")}
          </button>
        </div>
      </header>

      <main className="overview">
        <section className="overview-intro">
          <p className="overview-kicker">{t("overviewKicker")}</p>
          <h1>{t("overviewTitle")}</h1>
          <p>{t("overviewDescription")}</p>
        </section>

        <section className="launch-panel" aria-labelledby="start-scan-heading">
          <div className="launch-panel__heading">
            <div>
              <h2 id="start-scan-heading">{t("startScan")}</h2>
              <p>{t("startScanDescription")}</p>
            </div>
            <ShieldCheck size={25} weight="duotone" aria-hidden="true" />
          </div>

          <div className="quick-actions">
            <button className="quick-action" onClick={() => onScan(homePath)}>
              <span className="quick-action__icon">
                <House size={22} weight="duotone" />
              </span>
              <span>
                <strong>{t("homeFolder")}</strong>
                <small dir="auto">{homePath}</small>
              </span>
              <ArrowRight size={18} />
            </button>
            <button className="quick-action" onClick={() => onScan("/")}>
              <span className="quick-action__icon">
                <HardDrive size={22} weight="duotone" />
              </span>
              <span>
                <strong>{t("rootFilesystem")}</strong>
                <small>/</small>
              </span>
              <ArrowRight size={18} />
            </button>
            <button className="quick-action" onClick={onChooseFolder}>
              <span className="quick-action__icon">
                <Folder size={22} weight="duotone" />
              </span>
              <span>
                <strong>{t("chooseFolder")}</strong>
                <small>{t("localOrRemovable")}</small>
              </span>
              <ArrowRight size={18} />
            </button>
            <button className="quick-action" onClick={onRemote}>
              <span className="quick-action__icon">
                <LinkSimple size={22} weight="duotone" />
              </span>
              <span>
                <strong>{t("remoteLocation")}</strong>
                <small>{t("remoteLocationDescription")}</small>
              </span>
              <ArrowRight size={18} />
            </button>
          </div>

          <div className="admin-callout">
            <span className="admin-callout__icon" aria-hidden="true">
              <LockKey size={24} weight="duotone" />
            </span>
            <div>
              <strong>{t("needCompleteAccess")}</strong>
              <p>{t("administratorDescription")}</p>
            </div>
            <button className="button button--primary" onClick={onAdmin}>
              {t("scanAsAdministrator")}
            </button>
          </div>
        </section>

        <section className="volumes-section" aria-labelledby="storage-heading">
          <div className="section-heading">
            <div>
              <h2 id="storage-heading">{t("storage")}</h2>
              <p>{t("storageDescription")}</p>
            </div>
          </div>

          {volumesLoading ? (
            <div
              className="volume-skeletons"
              aria-label={t("loadingStorage")}
            >
              <div className="skeleton skeleton--volume" />
              <div className="skeleton skeleton--volume" />
            </div>
          ) : volumes.length === 0 ? (
            <div className="empty-inline">{t("noStorage")}</div>
          ) : (
            <div className="volume-list">
              {volumes.map((volume) => {
                const capacity = getVolumeCapacity(volume);
                const percentage = formatPercentage(
                  capacity.usedPercent,
                  locale
                );
                const volumeName =
                  volume.mountPoint === "/" && volume.name === "System"
                    ? t("systemFilesystem")
                    : volume.name || volume.mountPoint;
                return (
                  <button
                    className="volume-row"
                    key={`${volume.mountPoint}-${volume.name}`}
                    onClick={() => onScan(volume.mountPoint)}
                  >
                    <span className="volume-row__icon">
                      <HardDrive size={21} weight="duotone" />
                    </span>
                    <span className="volume-row__identity">
                      <strong>{volumeName}</strong>
                      <small dir="auto">
                        {volume.mountPoint} ·{" "}
                        {volume.fileSystem || t("filesystem")}
                      </small>
                    </span>
                    <span className="volume-row__usage">
                      <span className="volume-row__usage-head">
                        <span>
                          {formatBytes(
                            capacity.usedBytes,
                            byteUnitScale,
                            locale
                          )}{" "}
                          {t("used").toLocaleLowerCase(locale)}
                        </span>
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
                      <span className="volume-row__usage-foot">
                        <span>
                          {formatBytes(
                            capacity.freeBytes,
                            byteUnitScale,
                            locale
                          )}{" "}
                          {t("free").toLocaleLowerCase(locale)}
                        </span>
                        <span>
                          {formatBytes(
                            capacity.totalBytes,
                            byteUnitScale,
                            locale
                          )}{" "}
                          {t("total").toLocaleLowerCase(locale)}
                        </span>
                      </span>
                    </span>
                    <ArrowRight size={17} />
                  </button>
                );
              })}
            </div>
          )}
        </section>
      </main>

      <footer className="overview-footer">
        <span>{t("privacyNotice")}</span>
        <span>GPL-3.0-or-later</span>
      </footer>
    </div>
  );
}
