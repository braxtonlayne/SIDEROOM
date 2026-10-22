// The reason picker of a platform ban (N-25; MOD-BAN-002; spec 8, D-211): the `reason_category` and the severity it
// presets. Serious by default for illegal content, child safety, fraud and platform abuse; every other reason
// presets standard, and `harassment` and `ban_evasion` can be raised with the toggle. Shared by the case page and
// the Accounts ban dialog.
import { Checkbox, Select } from '@sideroom/ui';
import { useT } from '../../i18n';
import { CATEGORY_KEYS } from '../../moderation/ReportDialog';
import * as api from '../../moderation/api';

/** The severity a reason starts with (the stored severity is what the invite graph reads, D-031). */
export const presetSeverity = (c: api.ReportCategory): api.Severity =>
  ['illegal_content', 'child_safety', 'fraud', 'platform_abuse'].includes(c)
    ? 'serious'
    : 'standard';

export function BanReasonFields({
  reason,
  severity,
  onChange,
}: {
  reason: api.ReportCategory;
  severity: api.Severity;
  onChange: (reason: api.ReportCategory, severity: api.Severity) => void;
}) {
  const t = useT();
  return (
    <>
      <Select
        aria-label={t('admin.accounts.banReasonLabel')}
        options={api.REPORT_CATEGORIES.map((c) => ({ value: c, label: t(CATEGORY_KEYS[c]) }))}
        value={reason}
        onValueChange={(v) => {
          const c = v as api.ReportCategory;
          onChange(c, presetSeverity(c));
        }}
      />
      <Checkbox
        label={t('admin.accounts.banSeriousToggle')}
        description={t('admin.accounts.banSeriousToggleHint')}
        checked={severity === 'serious'}
        onChange={(e) => onChange(reason, e.target.checked ? 'serious' : 'standard')}
      />
    </>
  );
}
