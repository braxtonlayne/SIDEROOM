// Minimal bucket-scoped policy examples shown next to the connect form (BYOS-UI-001, review note 2).
// They grant exactly what the connection test runs: list, get, put, delete and multipart on one bucket
// and prefix, plus reading and writing the bucket CORS rule. Code samples, not UI strings.
import type { MountProvider } from './types';

/** IAM-style JSON for providers that use policy documents; null for token-scope providers (R2, B2, Garage). */
export function policyExample(
  provider: MountProvider,
  bucket: string,
  prefix: string,
): string | null {
  if (provider !== 's3' && provider !== 'wasabi' && provider !== 'minio') return null;
  const b = bucket.trim() || 'YOUR-BUCKET';
  const p = prefix.trim().replace(/^\/+|\/+$/g, '');
  const arn = 'arn:aws:s3:::';
  const objects = `${arn}${b}/${p ? `${p}/` : ''}*`;
  const doc = {
    Version: '2012-10-17',
    Statement: [
      {
        Effect: 'Allow',
        Action: [
          's3:ListBucket',
          's3:GetBucketCORS',
          's3:PutBucketCORS',
          's3:ListBucketMultipartUploads',
        ],
        Resource: `${arn}${b}`,
      },
      {
        Effect: 'Allow',
        Action: [
          's3:GetObject',
          's3:PutObject',
          's3:DeleteObject',
          's3:AbortMultipartUpload',
          's3:ListMultipartUploadParts',
        ],
        Resource: objects,
      },
    ],
  };
  return JSON.stringify(doc, null, 2);
}
