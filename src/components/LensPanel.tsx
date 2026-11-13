import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { ArrowRight, Check, Copy, MapPin, Warning, X } from "@phosphor-icons/react";
import type {
  ByteUnitScale,
  DiffRow,
  ReclaimMatch,
  Reclaimable,
  ScanDiff
} from "../types";
import { formatBytes, formatCount } from "../lib/format";
import { formatDelta, splitDiff } from "../lib/lens";
import { useI18n, type MessageKey } from "../lib/i18n";

export function formatDate(seconds: number, locale: string) {
  return new Intl.DateTimeFormat(locale === "ar" ? "ar-u-nu-arab" : locale, {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(new Date(seconds * 1000));
}

function PanelHeading({
  title,
  subtitle,
  closeLabel,
  onClose
}: {
  title: string;
  subtitle?: string;
  closeLabel: string;
  onClose: () => void;
}) {
  return (
    <div className="lens-heading">
      <div>
        <h2>{title}</h2>
        {subtitle && <p>{subtitle}</p>}
      </div>
      <button className="icon-button" aria-label={closeLabel} onClick={onClose}>
        <X size={18} />
      </button>
    </div>
  );
}

function ChangeList({
  title,
  empty,
  rows,
  tone,
  scale,
  onShow
}: {
  title: string;
  empty: string;
  rows: DiffRow[];
  tone: "grow" | "shrink";
  scale: ByteUnitScale;
  onShow: (nodeId: number) => void;
}) {
  const { locale, t } = useI18n();
  return (
    <section className="lens-section" aria-label={title}>
      <h3>{title}</h3>
      {rows.length === 0 ? (
        <p className="lens-empty">{empty}</p>
      ) : (
        <ul className="lens-list">
          {rows.map((row) => (
            <li key={row.path}>
              <button
                className="lens-row"
                disabled={row.nodeId === null}
                onClick={() => row.nodeId !== null && onShow(row.nodeId)}
                title={row.nodeId === null ? row.path : `${t("showOnMap")}: ${row.path}`}
              >
                <span className="lens-row__path" dir="auto">
                  {row.path}
                </span>
                <strong className={`lens-delta lens-delta--${tone}`} dir="ltr">
                  {formatDelta(row.delta, scale, locale)}
                </strong>
                <small className="lens-row__sizes" dir="ltr">
                  {formatBytes(row.before, scale, locale)}
                  <ArrowRight size={10} aria-hidden="true" />
                  {formatBytes(row.after, scale, locale)}
                </small>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function ComparePanel({
  diff,
  scale,
  onShow,
  onExit
}: {
  diff: ScanDiff;
  scale: ByteUnitScale;
  onShow: (nodeId: number) => void;
  onExit: () => void;
}) {
  const { locale, t } = useI18n();
  const { grew, shrank } = splitDiff(diff.rows);
  return (
    <aside className="detail-panel lens-panel" aria-label={t("whatChanged")}>
      <PanelHeading
        title={t("whatChanged")}
        subtitle={t("changedSince", { date: formatDate(diff.beforeTimestamp, locale) })}
        closeLabel={t("leaveComparison")}
        onClose={onExit}
      />
      <dl className="detail-stats">
        <div>
          <dt>{t("cmpBefore")}</dt>
          <dd>{formatBytes(diff.beforeTotal, scale, locale)}</dd>
        </div>
        <div>
          <dt>{t("cmpAfter")}</dt>
          <dd>{formatBytes(diff.afterTotal, scale, locale)}</dd>
        </div>
        <div>
          <dt>{t("cmpChange")}</dt>
          <dd dir="ltr">{formatDelta(diff.delta, scale, locale)}</dd>
        </div>
      </dl>
      <ChangeList
        title={t("cmpGrew")}
        empty={t("cmpNoneGrew")}
        rows={grew}
        tone="grow"
        scale={scale}
        onShow={onShow}
      />
      <ChangeList
        title={t("cmpShrank")}
        empty={t("cmpNoneShrank")}
        rows={shrank}
        tone="shrink"
        scale={scale}
        onShow={onShow}
      />
      <button className="button button--quiet-border button--full" onClick={onExit}>
        {t("leaveComparison")}
      </button>
    </aside>
  );
}

function ReclaimItem({
  match,
  scale,
  onShow,
  onNotify
}: {
  match: ReclaimMatch;
  scale: ByteUnitScale;
  onShow: (nodeId: number) => void;
  onNotify: (message: string, tone?: "normal" | "error") => void;
}) {
  const { locale, t } = useI18n();
  return (
    <li className="reclaim-item">
      <div className="reclaim-item__head">
        <strong>{t(match.titleKey as MessageKey)}</strong>
        <span>{formatBytes(match.sizeBytes, scale, locale)}</span>
      </div>
      <p className="reclaim-item__path" dir="auto">
        {match.path}
      </p>
      <p>{t(match.explanationKey as MessageKey)}</p>
      {match.command && (
        <div className="command-block">
          {match.warning && (
            <p className="command-warning">
              <Warning size={13} weight="fill" /> {t("reclaimWarning")}
            </p>
          )}
          <span className="command-label">{t("reclaimCommand")}</span>
          <div className="command-box">
            <code className="mono" dir="ltr">
              {match.command}
            </code>
            <button
              className="button button--quiet-border"
              onClick={() => {
                const command = match.command ?? "";
                writeText(command)
                  .then(() => onNotify(t("commandCopied")))
                  .catch((error) => onNotify(String(error), "error"));
              }}
            >
              <Copy size={14} />
              {t("copy")}
            </button>
          </div>
        </div>
      )}
      <button
        className="button button--quiet"
        onClick={() => onShow(match.nodeId)}
      >
        <MapPin size={15} />
        {t("showOnMap")}
      </button>
    </li>
  );
}

export function ReclaimPanel({
  data,
  scale,
  onShow,
  onExit,
  onNotify
}: {
  data: Reclaimable;
  scale: ByteUnitScale;
  onShow: (nodeId: number) => void;
  onExit: () => void;
  onNotify: (message: string, tone?: "normal" | "error") => void;
}) {
  const { locale, t } = useI18n();
  return (
    <aside className="detail-panel lens-panel" aria-label={t("reclaimable")}>
      <PanelHeading
        title={t("reclaimable")}
        closeLabel={t("close")}
        onClose={onExit}
      />
      {data.matches.length === 0 ? (
        <p className="lens-empty" role="status">
          <Check size={14} /> {t("reclaimNone")}
        </p>
      ) : (
        <>
          <p className="reclaim-total" role="status">
            {t("reclaimTotal", {
              size: formatBytes(data.totalBytes, scale, locale)
            })}
          </p>
          <p className="lens-note">{t("reclaimIntro")}</p>
          {data.matches.some((match) => !match.counted) && (
            <p className="lens-note">{t("reclaimUncertainNote")}</p>
          )}
          <ul className="lens-list lens-list--reclaim">
            {data.matches.map((match) => (
              <ReclaimItem
                key={match.path}
                match={match}
                scale={scale}
                onShow={onShow}
                onNotify={onNotify}
              />
            ))}
          </ul>
          {data.more > 0 && (
            <p className="lens-note">
              {t("moreItems", { count: formatCount(data.more, locale) })}
            </p>
          )}
        </>
      )}
    </aside>
  );
}
