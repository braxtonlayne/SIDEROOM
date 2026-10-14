// Chat tab. Messages here are the party's own, short-lived chat (delivered live, never stored). A party bound to
// a channel also links to that channel's full chat in the Lounge, which keeps the voice call and the mini player
// running while the viewer is there.
import { Button, TextInput } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { partyController, useParty } from './runtime';

export function ChatPanel() {
  const c = partyController();
  const router = useRouter();
  const party = useParty((s) => s.party);
  const lines = useParty((s) => s.messages);
  const [text, setText] = useState('');
  const [error, setError] = useState<string | null>(null);
  const end = useRef<HTMLLIElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView?.({ block: 'end' });
  }, [lines.length]);
  if (!party) return null;
  const name = (id: string) => {
    const p = party.participants.find((x) => x.user_id === id);
    return p ? p.display_name || p.username : 'Someone';
  };
  const send = async () => {
    const t = text.trim();
    if (!t) return;
    setError(null);
    try {
      await c.say(t);
      setText('');
    } catch {
      setError('Could not send. Slow down a little.');
    }
  };
  const channelHref =
    party.channel_id && party.guild_id
      ? `/channels/${party.guild_id}/${party.channel_id}`
      : party.channel_id
        ? `/channels/@me/${party.channel_id}`
        : null;
  return (
    <div className="wp-chat">
      <ul className="wp-chat-lines" aria-label="Party chat" aria-live="polite">
        {lines.length === 0 && <li className="wp-muted">Say something to the room.</li>}
        {lines.map((l) => (
          <li key={l.id} className="wp-line">
            <strong>{name(l.userId)}</strong> {l.content}
          </li>
        ))}
        <li ref={end} aria-hidden="true" />
      </ul>
      <form
        className="wp-chat-form"
        onSubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        <TextInput
          label="Message"
          value={text}
          maxLength={500}
          onChange={(e) => setText(e.target.value)}
          placeholder="Message the party"
        />
        <Button type="submit" size="sm" disabled={!text.trim()}>
          Send
        </Button>
      </form>
      {error && (
        <p role="alert" className="wp-error">
          {error}
        </p>
      )}
      <p className="wp-muted">Party chat is not saved.</p>
      {channelHref && (
        <Button size="sm" variant="secondary" onClick={() => router.history.push(channelHref)}>
          Open the channel chat
        </Button>
      )}
    </div>
  );
}
