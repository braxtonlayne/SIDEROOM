import type { t as translate } from '../../i18n/runtime';
import { COUNT_CAP } from './threadStore';

/**
 * "1 message", "12 messages", "50+ messages" (D-718): the one wording of a thread's count, used by the chip under a
 * message, the "started a thread" row, the Threads list and a forum post's row. At the server's cap
 * (`COUNT_CAP`) the true number is not known, so it reads "50+".
 */
export function threadCountLabel(t: typeof translate, n: number): string {
  return n >= COUNT_CAP
    ? t('lounge.threads.messageCountCapped', { count: COUNT_CAP })
    : t('lounge.threads.messageCount', { count: n });
}
