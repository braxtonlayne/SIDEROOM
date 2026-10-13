/**
 * Starter layouts offered in "Create a Server" (LNG-GLD-015). The backend has no template-based creation
 * yet (LNG-GLD-003), so after `POST /guilds` makes the server with its default channels, the client adds
 * the template's extra categories and channels one by one.
 */
export interface TemplateChannel {
  name: string;
  /** 0 text, 2 voice. */
  type: 0 | 2;
}
export interface TemplateCategory {
  name: string;
  channels: TemplateChannel[];
}
export interface ServerTemplate {
  id: string;
  label: string;
  /** Short line shown under the label. */
  blurb: string;
  categories: TemplateCategory[];
}

const text = (name: string): TemplateChannel => ({ name, type: 0 });
const voice = (name: string): TemplateChannel => ({ name, type: 2 });

export const SERVER_TEMPLATES: ServerTemplate[] = [
  {
    id: 'gaming',
    label: 'Gaming',
    blurb: 'Squads, clips and voice lobbies',
    categories: [
      {
        name: 'Games',
        channels: [text('game-chat'), text('clips-and-highlights'), voice('Squad 1')],
      },
    ],
  },
  {
    id: 'friends',
    label: 'Friends',
    blurb: 'A hangout for your close circle',
    categories: [{ name: 'Hangout', channels: [text('memes'), text('photos'), voice('Hangout')] }],
  },
  {
    id: 'study',
    label: 'Study Group',
    blurb: 'Notes, questions and quiet rooms',
    categories: [
      { name: 'Study', channels: [text('homework-help'), text('resources'), voice('Study Room')] },
    ],
  },
  {
    id: 'school-club',
    label: 'School Club',
    blurb: 'Announcements, events and meetings',
    categories: [
      { name: 'Club', channels: [text('announcements'), text('events'), voice('Meeting Room')] },
    ],
  },
  {
    id: 'local',
    label: 'Local Community',
    blurb: 'Neighbors, meetups and local news',
    categories: [
      { name: 'Community', channels: [text('announcements'), text('meetups'), text('local-news')] },
    ],
  },
  {
    id: 'creators',
    label: 'Artists & Creators',
    blurb: 'Share work and get feedback',
    categories: [
      { name: 'Studio', channels: [text('showcase'), text('feedback'), voice('Co-working')] },
    ],
  },
  {
    id: 'watch-club',
    label: 'Watch Club',
    blurb: 'Movies, shows and watch nights',
    categories: [
      {
        name: 'Watching',
        channels: [text('now-watching'), text('recommendations'), voice('Watch Night')],
      },
    ],
  },
  {
    id: 'reading-club',
    label: 'Reading Club',
    blurb: 'Book picks and discussion',
    categories: [
      {
        name: 'Reading',
        channels: [text('current-read'), text('recommendations'), voice('Book Chat')],
      },
    ],
  },
];

export const templateById = (id: string) => SERVER_TEMPLATES.find((t) => t.id === id);
