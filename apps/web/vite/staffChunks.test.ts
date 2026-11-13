import { describe, expect, it } from 'vitest';
import { sortModules, staffChunks, type ModuleNode } from './staffChunks';

const g =
  (o: Record<string, [string[], string[]?]>) =>
  (id: string): ModuleNode | null =>
    o[id] ? { importedIds: o[id]![0], dynamicallyImportedIds: o[id]![1] ?? [] } : null;
const isStaff = (id: string) => id === '/src/admin/AdminApp.tsx';

describe('sortModules (SEC-AUD-170)', () => {
  const graph = g({
    '/src/main.tsx': [['/src/shared.ts'], ['/src/admin/StaffDoor.tsx', '/src/lounge.tsx']],
    '/src/admin/StaffDoor.tsx': [[], ['/src/admin/AdminApp.tsx']],
    '/src/lounge.tsx': [['/src/shared.ts']],
    '/src/admin/AdminApp.tsx': [
      ['/src/admin/Queue.tsx', '/src/shared.ts'],
      ['/src/admin/Lazy.tsx'],
    ],
    '/src/admin/Queue.tsx': [['/src/admin/reauth.ts']],
    '/src/admin/Lazy.tsx': [[]],
  });
  const all = Object.keys({
    '/src/main.tsx': 1,
    '/src/shared.ts': 1,
    '/src/admin/StaffDoor.tsx': 1,
    '/src/lounge.tsx': 1,
    '/src/admin/AdminApp.tsx': 1,
    '/src/admin/Queue.tsx': 1,
    '/src/admin/Lazy.tsx': 1,
    '/src/admin/reauth.ts': 1,
  });

  it('a module only a staff entry reaches is staff; one that member code also imports stays a member', () => {
    const r = sortModules(all, ['/src/main.tsx'], graph, isStaff);
    expect([...r.staff].sort()).toEqual([
      '/src/admin/AdminApp.tsx',
      '/src/admin/Lazy.tsx',
      '/src/admin/Queue.tsx',
      '/src/admin/reauth.ts',
    ]);
    expect(r.member.has('/src/shared.ts')).toBe(true);
    expect(r.member.has('/src/admin/StaffDoor.tsx')).toBe(true);
  });

  it('a shared admin helper that member code imports is a member', () => {
    const withShared = g({
      '/src/main.tsx': [['/src/admin/reauth.ts'], ['/src/admin/AdminApp.tsx']],
      '/src/admin/AdminApp.tsx': [['/src/admin/reauth.ts']],
    });
    const r = sortModules(
      ['/src/main.tsx', '/src/admin/reauth.ts', '/src/admin/AdminApp.tsx'],
      ['/src/main.tsx'],
      withShared,
      isStaff,
    );
    expect([...r.staff]).toEqual(['/src/admin/AdminApp.tsx']);
  });

  it('a static import of a staff entry from member code fails, naming the importer', () => {
    const bad = g({
      '/src/main.tsx': [['/src/lounge.tsx']],
      '/src/lounge.tsx': [['/src/admin/AdminApp.tsx']],
    });
    expect(() => sortModules(['/src/main.tsx'], ['/src/main.tsx'], bad, isStaff)).toThrow(
      /\/src\/lounge\.tsx/,
    );
  });
});

describe('chunk names', () => {
  it('before the graph is known every chunk keeps the default pattern', () => {
    const s = staffChunks();
    expect(s.chunkFileNames({ moduleIds: ['/x'] })).toBe('[name]-[hash].js');
    expect(s.assetFileNames({ originalFileNames: ['src/a.css'] })).toBe('[name]-[hash][extname]');
  });
});
