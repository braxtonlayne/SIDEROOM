export interface S3Config {
  endpoint: string;
  bucket: string;
  region: string;
  ak: string;
  sk: string;
}
export function s3FromEnv(env?: Record<string, string | undefined>): S3Config;
export function presignUrl(
  S3: S3Config,
  method: string,
  key: string,
  query?: Record<string, string>,
  expires?: number,
): string;
export function listParts(
  S3: S3Config,
  key: string,
  uploadId: string,
): Promise<{ part: number; size: number }[] | null>;
export function listUploads(
  S3: S3Config,
  prefix?: string,
): Promise<{ key: string; uploadId: string }[]>;
