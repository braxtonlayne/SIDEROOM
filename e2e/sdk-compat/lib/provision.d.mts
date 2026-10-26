// Types for provision.mjs, so the TypeScript gate specs can use it (RDY-DEV-005).
export class Person {
  constructor(base: string, origin?: string);
  base: string;
  origin: string;
  call<T = unknown>(method: string, path: string, body?: unknown): Promise<T>;
  get<T = unknown>(path: string): Promise<T>;
  post<T = unknown>(path: string, body?: unknown): Promise<T>;
  patch<T = unknown>(path: string, body?: unknown): Promise<T>;
  login(username: string, password: string): Promise<void>;
}
export function waitReady(base: string, seconds?: number): Promise<void>;
export function installBot(
  person: Person,
  guildId: string,
  name: string,
  permissions: string,
): Promise<{ appId: string; token: string }>;
