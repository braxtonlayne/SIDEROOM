import { useT } from '../../i18n';
import { MAX_GADGETS } from '../gadgets/layout';
import type { ToolProps } from '../registry';

/** The drawer's own guide: the one tool the shell ships, so the drawer is never empty. */
export default function GuideTool(_props: ToolProps) {
  const t = useT();
  return (
    <ul className="sr-tl-guide">
      <li>{t('tools.guide.openDrawer')}</li>
      <li>{t('tools.guide.typeArrow')}</li>
      <li>{t('tools.guide.popOut', { max: MAX_GADGETS })}</li>
      <li>{t('tools.guide.pin')}</li>
      <li>{t('tools.guide.escape')}</li>
    </ul>
  );
}
