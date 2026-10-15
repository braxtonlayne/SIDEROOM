// Poll templates in the poll modal (FTR-POLLT-001): pick a saved setup to fill the form; people who manage the server
// can also save the current form as a template. Renders nothing unless the server runs the module.
import { Button, Select, TextInput } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n';
import { useGuild } from '../data';
import {
  createPollTemplate,
  listPollTemplates,
  type PollTemplate,
  type PollTemplateInput,
} from '../features/api';
import { MODULE, useGuildModules } from '../features/useModules';

export interface TemplateFill {
  question: string;
  answers: string[];
  hours: number;
  multi: boolean;
}

export const fillOf = (tpl: PollTemplate): TemplateFill => ({
  question: tpl.question,
  answers: tpl.answers,
  hours: tpl.duration_hours,
  multi: tpl.allow_multiselect,
});

export function PollTemplates({
  guildId,
  current,
  onPick,
}: {
  guildId: string;
  /** The form as it stands, for "save as template". */
  current: TemplateFill;
  onPick: (fill: TemplateFill) => void;
}) {
  const t = useT();
  const modules = useGuildModules(guildId);
  const manage = !!useGuild(guildId)?.permissions.manageGuild;
  const on = !!modules?.[MODULE.pollTemplates];
  const [templates, setTemplates] = useState<PollTemplate[]>([]);
  const [name, setName] = useState('');
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<{ text: string; bad: boolean } | null>(null);

  useEffect(() => {
    if (!on) return;
    let live = true;
    listPollTemplates(guildId)
      .then((list) => live && setTemplates(list))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [guildId, on]);

  if (!on) return null;

  const save = async () => {
    const input: PollTemplateInput = {
      name: name.trim(),
      question: current.question.trim(),
      answers: current.answers.map((a) => a.trim()).filter(Boolean),
      allow_multiselect: current.multi,
      duration_hours: current.hours,
    };
    setSaving(true);
    setMessage(null);
    try {
      const created = await createPollTemplate(guildId, input);
      setTemplates((l) => [...l, created]);
      setName('');
      setMessage({ text: t('lounge.pollTemplates.saved'), bad: false });
    } catch (e) {
      setMessage({
        text: e instanceof ApiError && e.message ? e.message : t('lounge.pollTemplates.failed'),
        bad: true,
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="lng-poll-templates">
      {templates.length > 0 && (
        <Select
          aria-label={t('lounge.pollTemplates.pick')}
          placeholder={t('lounge.pollTemplates.pick')}
          options={templates.map((x) => ({ value: x.id, label: x.name }))}
          value={null}
          onValueChange={(id) => {
            const tpl = templates.find((x) => x.id === id);
            if (tpl) onPick(fillOf(tpl));
          }}
        />
      )}
      {manage && (
        <div className="lng-poll-save">
          <TextInput
            label={t('lounge.pollTemplates.name')}
            value={name}
            maxLength={60}
            onChange={(e) => setName(e.target.value)}
          />
          <Button
            size="sm"
            variant="secondary"
            disabled={saving || !name.trim() || !current.question.trim()}
            onClick={() => void save()}
          >
            {t('lounge.pollTemplates.save')}
          </Button>
        </div>
      )}
      {message && (
        <p
          className={message.bad ? 'lng-failed' : 'lng-muted'}
          role={message.bad ? 'alert' : 'status'}
        >
          {message.text}
        </p>
      )}
    </div>
  );
}
