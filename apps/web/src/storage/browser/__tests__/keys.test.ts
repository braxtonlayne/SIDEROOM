import { describe, expect, it } from 'vitest';
import { baseName, editablePath, folderKey, moveTarget, uploadKey, validName } from '../keys';

describe('keys (BYOS-BRW-001)', () => {
  it('builds the upload key from the folder being viewed and the picked path', () => {
    expect(uploadKey('', 'a.mp3')).toBe('a.mp3');
    expect(uploadKey('Music/', 'Album/01 intro.flac')).toBe('Music/Album/01 intro.flac');
    expect(uploadKey('Music/', './x.mp3')).toBe('Music/x.mp3');
  });

  it('refuses names the server would refuse', () => {
    for (const bad of ['', '../x', 'a//b', 'a/./b', '/abs', 'a\\b', 'a\u0007b', 'dir/']) {
      expect(uploadKey('Music/', bad), bad).toBeNull();
    }
    expect(uploadKey('', 'x'.repeat(1025))).toBeNull();
    expect(uploadKey('', 'é'.repeat(600))).toBeNull(); // 1200 bytes
  });

  it('makes folder keys with a trailing slash and no slash in the name', () => {
    expect(folderKey('Music/', ' New ')).toBe('Music/New/');
    expect(folderKey('', 'a/b')).toBeNull();
    expect(folderKey('', '..')).toBeNull();
    expect(folderKey('', '   ')).toBeNull();
  });

  it('keeps a folder a folder when it is renamed or moved', () => {
    expect(editablePath('Music/Live/')).toBe('Music/Live');
    expect(moveTarget('Music/Live/', 'Records/Live2')).toBe('Records/Live2/');
    expect(moveTarget('Music/a.mp3', ' Music//b.mp3 ')).toBe('Music/b.mp3');
    expect(moveTarget('Music/a.mp3', '../b.mp3')).toBeNull();
    expect(moveTarget('Music/a.mp3', '')).toBeNull();
  });

  it('names things for the screen', () => {
    expect(baseName('Music/Live/')).toBe('Live');
    expect(baseName('a.mp3')).toBe('a.mp3');
    expect(validName('ok.txt')).toBe(true);
    expect(validName('a/b')).toBe(false);
  });
});
