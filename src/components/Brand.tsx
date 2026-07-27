import { ShieldCheck } from "@phosphor-icons/react";
import { useI18n } from "../lib/i18n";

export function LiScanMark({ compact = false }: { compact?: boolean }) {
  return (
    <div className={`brand-lockup ${compact ? "brand-lockup--compact" : ""}`}>
      <span className="brand-mark" aria-hidden="true">
        <span className="brand-mark__ring brand-mark__ring--outer" />
        <span className="brand-mark__ring brand-mark__ring--middle" />
        <span className="brand-mark__core" />
      </span>
      <span className="brand-name">LiScan</span>
    </div>
  );
}
export function AdminBadge() {
  const { t } = useI18n();
  return (
    <span className="admin-badge">
      <ShieldCheck size={14} weight="fill" />
      {t("administratorScan")}
    </span>
  );
}
