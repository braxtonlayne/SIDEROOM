// "New case" (N-22; MOD-CASE-007): open a case about an account without a report. The first note is required,
// so a case never starts blank.
import { Select, TextArea, TextInput } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { CATEGORY_KEYS } from '../../moderation/ReportDialog';
import * as api from '../../moderation/api';
import { FormDialog } from '../ui';
import { SEVERITY_KEYS } from './labels';

const presetSeverity = (c: api.ReportCategory): api.Severity =>
  ['illegal_content', 'child_safety', 'fraud', 'platform_abuse'].includes(c)
    ? 'serious'
    : 'standard';

export function NewCaseDialog({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (id: string) => void;
}) {
  const t = useT();
  const [account, setAccount] = useState('');
  const [guild, setGuild] = useState('');
  const [category, setCategory] = useState<api.ReportCategory>('other');
  const [severity, setSeverity] = useState<api.Severity>('standard');
  const [note, setNote] = useState('');
  const idOk = (s: string) => /^\d+$/.test(s.trim());
  const valid = idOk(account) && (guild.trim() === '' || idOk(guild)) && note.trim().length > 0;
  return (
    <FormDialog
      open={open}
      title={t('admin.moderation.newCase.title')}
      description={t('admin.moderation.newCase.description')}
      confirmLabel={t('admin.moderation.newCase.confirm')}
      valid={valid}
      onCancel={onClose}
      onConfirm={async () => {
        const made = await api.openCase({
          category,
          target_kind: 'user',
          target_id: account.trim(),
          subject_account_id: account.trim(),
          severity,
          ...(guild.trim() ? { guild_id: guild.trim() } : {}),
        });
        await api.addCaseNote(made.id, note.trim());
        setAccount('');
        setGuild('');
        setNote('');
        onClose();
        onCreated(made.id);
      }}
    >
      <TextInput
        label={t('admin.moderation.newCase.account')}
        value={account}
        inputMode="numeric"
        onChange={(e) => setAccount(e.target.value)}
      />
      <TextInput
        label={t('admin.moderation.newCase.guild')}
        value={guild}
        inputMode="numeric"
        onChange={(e) => setGuild(e.target.value)}
      />
      <Select
        aria-label={t('admin.moderation.newCase.category')}
        options={api.REPORT_CATEGORIES.map((c) => ({ value: c, label: t(CATEGORY_KEYS[c]) }))}
        value={category}
        onValueChange={(v) => {
          const c = v as api.ReportCategory;
          setCategory(c);
          setSeverity(presetSeverity(c));
        }}
      />
      <Select
        aria-label={t('admin.moderation.newCase.severity')}
        options={[
          { value: 'serious', label: t(SEVERITY_KEYS.serious) },
          { value: 'standard', label: t(SEVERITY_KEYS.standard) },
        ]}
        value={severity}
        onValueChange={(v) => setSeverity(v as api.Severity)}
      />
      <TextArea
        label={t('admin.moderation.newCase.note')}
        value={note}
        rows={4}
        maxLength={4000}
        onChange={(e) => setNote(e.target.value)}
      />
    </FormDialog>
  );
}
