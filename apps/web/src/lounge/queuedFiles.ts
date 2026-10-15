/**
 * Files waiting to become composer attachments for a channel (G-18: the Share Target page picks a channel,
 * navigates there, and the channel's composer takes the files on mount). In memory only, one-shot.
 */
const queued = new Map<string, File[]>();

export const queuedFiles = {
  set(channelId: string, files: File[]) {
    if (files.length) queued.set(channelId, files);
  },
  /** Returns and forgets the files queued for this channel. */
  take(channelId: string): File[] {
    const f = queued.get(channelId) ?? [];
    queued.delete(channelId);
    return f;
  },
};
