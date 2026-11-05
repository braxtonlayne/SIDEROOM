// @ts-check
// Guild Prompt Packs (ACT-SKS-001, D-389, D-396). A pack is a small file on one of the guild's shelves that the
// HOST (a member) opens through SIDE ROOM's per-object URL: the member's page fetches it and hands the game the
// bytes (`shelfObjectBytes`, D-460); guests never load one. Two shapes:
//
//   JSON  { "name": "Office chaos", "prompts": ["stapler duel", "reply-all storm", ...] }   (or a bare array)
//   text  one prompt per line; blank lines and lines starting with # are skipped
//
// Everything is checked here before the game uses it: the size, the count, each prompt's length and characters.
// A pack that fails the checks is refused as a whole with a reason the host can read.

export const PACK_MAX_BYTES = 64 * 1024;
export const PACK_MAX_PROMPTS = 1000;
export const PACK_MIN_PROMPTS = 5;
export const PROMPT_MAX_CHARS = 48;
export const PACK_NAME_MAX_CHARS = 40;

/** Control and format characters, angle brackets and the bidi overrides go; whitespace collapses. @param {string} s */
export function cleanText(s) {
  return String(s ?? '')
    .replace(/[\p{C}<>]/gu, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * @typedef {{ ok: true, name: string, prompts: string[], skipped: number } | { ok: false, error: string }} PackResult
 */

/**
 * Parse and validate a pack from its bytes (or text).
 * @param {Uint8Array | string} data
 * @param {string} [fallbackName] the name to use when the file has none
 * @returns {PackResult}
 */
export function parsePack(data, fallbackName = 'Server pack') {
  const size = typeof data === 'string' ? new TextEncoder().encode(data).length : data.length;
  if (size === 0) return { ok: false, error: 'empty' };
  if (size > PACK_MAX_BYTES) return { ok: false, error: 'too-big' };
  let text;
  try {
    text = typeof data === 'string' ? data : new TextDecoder('utf-8', { fatal: true }).decode(data);
  } catch {
    return { ok: false, error: 'not-text' };
  }
  text = text.replace(/^﻿/, '');
  /** @type {unknown[]} */
  let raw;
  let name = fallbackName;
  const trimmed = text.trim();
  if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
    let json;
    try {
      json = JSON.parse(trimmed);
    } catch {
      return { ok: false, error: 'bad-json' };
    }
    if (Array.isArray(json)) raw = json;
    else if (json && typeof json === 'object' && Array.isArray(json.prompts)) {
      raw = json.prompts;
      if (typeof json.name === 'string' && cleanText(json.name)) name = cleanText(json.name);
    } else return { ok: false, error: 'no-prompts' };
  } else {
    raw = text.split(/\r?\n/).filter((l) => !l.trim().startsWith('#'));
  }
  const seen = new Set();
  /** @type {string[]} */
  const prompts = [];
  let skipped = 0;
  for (const item of raw) {
    if (typeof item !== 'string') {
      skipped++;
      continue;
    }
    const p = cleanText(item);
    if (p === '') continue;
    const key = p.toLowerCase();
    if (p.length > PROMPT_MAX_CHARS || !/[\p{L}\p{N}]/u.test(p) || seen.has(key)) {
      skipped++;
      continue;
    }
    seen.add(key);
    prompts.push(p);
    if (prompts.length > PACK_MAX_PROMPTS) return { ok: false, error: 'too-many' };
  }
  if (prompts.length < PACK_MIN_PROMPTS) return { ok: false, error: 'too-few' };
  return { ok: true, name: name.slice(0, PACK_NAME_MAX_CHARS), prompts, skipped };
}

/**
 * A shelf object named by the host: "<shelf id>/<object hash>", or any pasted text that holds a snowflake shelf id
 * followed by a lowercase SHA-256 (a shelf link, "shelf 123 file abc…").
 * @param {string} text @returns {{ shelf_id: string, object_hash: string } | null}
 */
export function parseShelfRef(text) {
  const m = /(?:^|\D)(\d{1,20})\D+?([0-9a-f]{64})(?![0-9a-f])/.exec(String(text ?? '').trim().toLowerCase());
  return m ? { shelf_id: m[1], object_hash: m[2] } : null;
}
