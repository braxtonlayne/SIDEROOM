// Read-only S3 helpers for the live storage checks (RDY-STO-001): SigV4 query presign (path-style) plus
// ListParts and ListMultipartUploads, so a test can compare what the app believes with what the store holds.
// Used by storage-live.mjs and tests/storage-composer.spec.ts. Plain http endpoints only (Garage / MinIO).
import { createHash, createHmac } from 'node:crypto';

const hmac = (k, s) => createHmac('sha256', k).update(s).digest();
const sha = (s) => createHash('sha256').update(s).digest('hex');
const enc = (s) =>
  encodeURIComponent(s).replace(
    /[!'()*]/g,
    (c) => '%' + c.charCodeAt(0).toString(16).toUpperCase(),
  );

/** The S3 settings of the live suites, from the SR_TEST_S3_* variables the storage workflow exports. */
export function s3FromEnv(env = process.env) {
  const need = (k) => {
    const v = env[k];
    if (!v) throw new Error(`${k} must be set`);
    return v;
  };
  return {
    endpoint: need('SR_TEST_S3_ENDPOINT').replace(/\/$/, ''),
    bucket: need('SR_TEST_S3_BUCKET'),
    region: env.SR_TEST_S3_REGION || 'garage',
    ak: need('SR_TEST_S3_ACCESS_KEY'),
    sk: need('SR_TEST_S3_SECRET_KEY'),
  };
}

/** A presigned URL for `method` on `key` ('' is the bucket itself); `query` adds signed parameters. */
export function presignUrl(S3, method, key, query = {}, expires = 60) {
  const u = new URL(S3.endpoint);
  const now = new Date().toISOString().replace(/[:-]|\.\d{3}/g, '');
  const date = now.slice(0, 8);
  const scope = `${date}/${S3.region}/s3/aws4_request`;
  const path = '/' + [S3.bucket, ...(key ? key.split('/') : [])].map(enc).join('/');
  const q = {
    ...query,
    'X-Amz-Algorithm': 'AWS4-HMAC-SHA256',
    'X-Amz-Credential': `${S3.ak}/${scope}`,
    'X-Amz-Date': now,
    'X-Amz-Expires': String(expires),
    'X-Amz-SignedHeaders': 'host',
  };
  const qs = Object.keys(q)
    .sort()
    .map((k) => `${enc(k)}=${enc(q[k])}`)
    .join('&');
  const canon = [method, path, qs, `host:${u.host}\n`, 'host', 'UNSIGNED-PAYLOAD'].join('\n');
  const sts = ['AWS4-HMAC-SHA256', now, scope, sha(canon)].join('\n');
  let k = hmac('AWS4' + S3.sk, date);
  for (const p of [S3.region, 's3', 'aws4_request']) k = hmac(k, p);
  const sig = createHmac('sha256', k).update(sts).digest('hex');
  return `${u.origin}${path}?${qs}&X-Amz-Signature=${sig}`;
}

const tag = (xml, name) =>
  [...xml.matchAll(new RegExp(`<${name}>([^<]*)</${name}>`, 'g'))].map((m) => m[1]);

/**
 * ListParts of one multipart upload: [{ part: 1, size: 67108864 }, ...] sorted by part number, or null when
 * the store no longer knows the upload (NoSuchUpload, i.e. aborted or completed).
 */
export async function listParts(S3, key, uploadId) {
  const parts = [];
  let marker = '0';
  for (;;) {
    const res = await fetch(presignUrl(S3, 'GET', key, { uploadId, 'part-number-marker': marker }));
    const xml = await res.text();
    if (res.status === 404 || /NoSuchUpload/.test(xml)) return null;
    if (res.status !== 200) throw new Error(`ListParts ${res.status}: ${xml.slice(0, 300)}`);
    const nums = tag(xml, 'PartNumber').map(Number);
    const sizes = tag(xml, 'Size').map(Number);
    nums.forEach((n, i) => parts.push({ part: n, size: sizes[i] }));
    if (!/<IsTruncated>true<\/IsTruncated>/.test(xml)) break;
    marker = tag(xml, 'NextPartNumberMarker')[0] ?? String(nums.at(-1));
  }
  return parts.sort((a, b) => a.part - b.part);
}

/** ListMultipartUploads under `prefix`: [{ key, uploadId }]. */
export async function listUploads(S3, prefix = '') {
  const res = await fetch(presignUrl(S3, 'GET', '', { uploads: '', prefix }));
  const xml = await res.text();
  if (res.status !== 200)
    throw new Error(`ListMultipartUploads ${res.status}: ${xml.slice(0, 300)}`);
  const blocks = xml.split('<Upload>').slice(1);
  return blocks.map((b) => ({ key: tag(b, 'Key')[0], uploadId: tag(b, 'UploadId')[0] }));
}
