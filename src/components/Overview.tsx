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
  return (
    <div className="overview-shell">
      <header className="overview-nav">
        <LiScanMark />
        <div className="overview-nav__actions">
          <button className="button button--quiet" onClick={onAbout}>
            About
          </button>
          <button className="button button--quiet" onClick={onSettings}>
            Settings
          </button>
        </div>
      </header>

      <main className="overview">
        <section className="overview-intro">
          <p className="overview-kicker">Disk usage, made clear</p>
          <h1>See where your disk space went.</h1>
          <p>
            Scan any Linux folder, inspect every level, and clean up with
            confidence.
          </p>
        </section>

        <section className="launch-panel" aria-labelledby="start-scan-heading">
          <div className="launch-panel__heading">
            <div>
              <h2 id="start-scan-heading">Start a scan</h2>
              <p>Choose a common location or browse to any folder.</p>
            </div>
            <ShieldCheck size={25} weight="duotone" aria-hidden="true" />
          </div>

          <div className="quick-actions">
            <button className="quick-action" onClick={() => onScan(homePath)}>
              <span className="quick-action__icon">
                <House size={22} weight="duotone" />
              </span>
              <span>
                <strong>Home folder</strong>
                <small>{homePath}</small>
              </span>
              <ArrowRight size={18} />
            </button>
            <button className="quick-action" onClick={() => onScan("/")}>
              <span className="quick-action__icon">
                <HardDrive size={22} weight="duotone" />
              </span>
              <span>
                <strong>Root filesystem</strong>
                <small>/</small>
              </span>
              <ArrowRight size={18} />
            </button>
            <button className="quick-action" onClick={onChooseFolder}>
              <span className="quick-action__icon">
                <Folder size={22} weight="duotone" />
              </span>
              <span>
                <strong>Choose folder</strong>
                <small>Local or removable storage</small>
              </span>
              <ArrowRight size={18} />
            </button>
            <button className="quick-action" onClick={onRemote}>
              <span className="quick-action__icon">
                <LinkSimple size={22} weight="duotone" />
              </span>
              <span>
                <strong>Remote location</strong>
                <small>SFTP, SMB, or another GIO location</small>
              </span>
              <ArrowRight size={18} />
            </button>
          </div>

          <div className="admin-callout">
            <span className="admin-callout__icon" aria-hidden="true">
              <LockKey size={24} weight="duotone" />
            </span>
            <div>
              <strong>Need complete access?</strong>
              <p>
                Authenticate once to include protected folders. LiScan itself
                stays unprivileged.
              </p>
            </div>
            <button className="button button--primary" onClick={onAdmin}>
              Scan as administrator
            </button>
          </div>
        </section>

        <section className="volumes-section" aria-labelledby="storage-heading">
          <div className="section-heading">
            <div>
              <h2 id="storage-heading">Storage</h2>
              <p>Mounted filesystems available to scan.</p>
            </div>
          </div>

          {volumesLoading ? (
            <div className="volume-skeletons" aria-label="Loading storage">
              <div className="skeleton skeleton--volume" />
              <div className="skeleton skeleton--volume" />
            </div>
          ) : volumes.length === 0 ? (
            <div className="empty-inline">
              No mounted storage could be detected. You can still choose a
              folder above.
            </div>
          ) : (
            <div className="volume-list">
              {volumes.map((volume) => {
                const capacity = getVolumeCapacity(volume);
                const percentage = formatPercentage(capacity.usedPercent);
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
                      <strong>{volume.name || volume.mountPoint}</strong>
                      <small>
                        {volume.mountPoint} · {volume.fileSystem || "filesystem"}
                      </small>
                    </span>
                    <span className="volume-row__usage">
                      <span className="volume-row__usage-head">
                        <span>
                          {formatBytes(capacity.usedBytes, byteUnitScale)} used
                        </span>
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
                      <span className="volume-row__usage-foot">
                        <span>
                          {formatBytes(capacity.freeBytes, byteUnitScale)} free
                        </span>
                        <span>
                          {formatBytes(capacity.totalBytes, byteUnitScale)} total
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
        <span>Local by design. No file data leaves your computer.</span>
        <span>GPL-3.0-or-later</span>
      </footer>
    </div>
  );
}
