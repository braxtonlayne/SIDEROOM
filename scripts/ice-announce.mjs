#!/usr/bin/env node
/**
 * Prints a `SR_VOICE__ANNOUNCE_IPS` value for browser e2e runs: the host's default-route IPv4 address, as a
 * one-element list (`["10.1.0.4"]`). Falls back to `["127.0.0.1"]` (with a warning) when there is none.
 *
 * Why: the dev SFU announces 127.0.0.1. Firefox pairs a loopback remote ICE candidate only with a loopback
 * local one, and it gathers loopback candidates only with `media.peerconnection.ice.loopback` and only when it
 * gathers on every interface. When Firefox does not count the page as permitted or capturing (Playwright
 * cannot grant camera/microphone permissions to Firefox), it gathers on the default-route interface alone.
 * Then no pair forms and ICE fails when the 5 s trickle grace period ends, which is what CI showed. The
 * default-route address is the one Firefox gathers in that mode, and Chromium and WebKit reach it too.
 *
 *   SR_VOICE__ANNOUNCE_IPS="$(node scripts/ice-announce.mjs)" ./deploy/compose/e2e-run.sh ...
 */
import dgram from 'node:dgram';
import { pathToFileURL } from 'node:url';

/** Any address in 127.0.0.0/8 (or IPv6 ::1). */
export function isLoopback(ip) {
  return ip === '::1' || /^127\./.test(ip);
}

/** The figment env form of a one-address list: a TOML-style array of strings. */
export function announceValue(ip) {
  return JSON.stringify([ip]);
}

/**
 * The source address the OS picks for traffic on the default route. A connected UDP socket sends nothing; it
 * only asks the routing table (Firefox finds its "default address" the same way). 192.0.2.1 is TEST-NET-1,
 * which no host routes specially, so the default route answers.
 */
export function defaultRouteAddress(
  target = '192.0.2.1',
  createSocket = () => dgram.createSocket('udp4'),
) {
  return new Promise((resolve) => {
    let sock;
    try {
      sock = createSocket();
    } catch {
      resolve(null);
      return;
    }
    const done = (ip) => {
      try {
        sock.close();
      } catch {
        /* already closed */
      }
      resolve(ip);
    };
    sock.once('error', () => done(null));
    try {
      sock.connect(53, target, () => {
        try {
          done(sock.address().address ?? null);
        } catch {
          done(null);
        }
      });
    } catch {
      done(null);
    }
  });
}

/** The list to announce: the default-route address, or loopback when there is no usable one. */
export async function announceIps(find = defaultRouteAddress) {
  const ip = await find();
  if (ip && !isLoopback(ip) && ip !== '0.0.0.0')
    return { value: announceValue(ip), routable: true };
  return { value: announceValue('127.0.0.1'), routable: false };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  const { value, routable } = await announceIps();
  if (!routable)
    console.error(
      'ice-announce: no default-route IPv4 address; announcing 127.0.0.1 (Firefox ICE will fail)',
    );
  console.log(value);
}
