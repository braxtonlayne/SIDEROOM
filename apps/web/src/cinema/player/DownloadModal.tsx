// Download modal (CIN-DL-002, spec cinema-media section 5): pick a rendition (sizes shown), follow the preparation
// job, then take the signed link. Patrons only: the server answers 403 for everyone else and the modal says so.
// The title page (browse UI) mounts it: <DownloadModal open titleId episodeId title onClose />.
import { Button, Modal, ProgressBar, Radio, RadioGroup } from '@sideroom/ui';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';
import { ApiError } from '../../api/client';
import { getDownloadJob, getDownloadOptions, startDownload } from './api';
import { formatBytes } from './model';
import type { DownloadJob } from './types';

export interface DownloadModalProps {
  open: boolean;
  onClose: () => void;
  titleId: string;
  episodeId?: string | null;
  /** For the heading, e.g. "Akira" or "Frieren · S1 E3". */
  title: string;
}

const POLL_MS = 2000;

const errorCopy = (e: unknown): string => {
  if (e instanceof ApiError) {
    if (e.status === 429) return 'You have reached today’s download limit. Try again tomorrow.';
    if (e.status === 403) return 'Downloads are for Patrons.';
  }
  return 'Something went wrong. Try again.';
};

export function DownloadModal({ open, onClose, titleId, episodeId, title }: DownloadModalProps) {
  const options = useQuery({
    queryKey: ['cinema', 'downloads', titleId, episodeId ?? null],
    queryFn: () => getDownloadOptions(titleId, episodeId),
    enabled: open,
    retry: false,
    staleTime: 0,
  });
  const [choice, setChoice] = useState<string>('');
  const [job, setJob] = useState<DownloadJob | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  const rows = useMemo(() => options.data?.renditions ?? [], [options.data]);
  useEffect(() => {
    if (!choice && rows.length) setChoice(rows[0]!.id);
  }, [rows, choice]);
  // Starting over every time the dialog opens.
  useEffect(() => {
    if (!open) {
      setJob(null);
      setFailure(null);
      setChoice('');
    }
  }, [open]);

  const start = useMutation({
    mutationFn: () => startDownload(titleId, choice, episodeId),
    onSuccess: (j) => {
      setFailure(null);
      setJob(j);
    },
    onError: (e) => setFailure(errorCopy(e)),
  });

  // Follow the prep job until it is ready or has failed.
  const jobId = job?.id;
  const active = job && (job.state === 'queued' || job.state === 'preparing');
  useEffect(() => {
    if (!jobId || !active) return;
    const id = setInterval(() => {
      getDownloadJob(jobId).then(setJob, (e) => {
        setFailure(errorCopy(e));
        clearInterval(id);
      });
    }, POLL_MS);
    return () => clearInterval(id);
  }, [jobId, active]);

  const notPatron = options.error instanceof ApiError && options.error.status === 403;

  let body;
  if (options.isPending) body = <p className="cpl-my-muted">Loading the available files…</p>;
  else if (notPatron)
    body = (
      <p role="alert">Downloads are for Patrons. Support SIDE ROOM to take titles with you.</p>
    );
  else if (options.isError) body = <p role="alert">We could not load the download options.</p>;
  else if (rows.length === 0) body = <p>There is nothing to download for this title yet.</p>;
  else if (job) {
    body = (
      <div>
        {(job.state === 'queued' || job.state === 'preparing') && (
          <>
            <p>{job.state === 'queued' ? 'Waiting in line…' : 'Preparing your file…'}</p>
            <ProgressBar value={Math.round(job.progress * 100)} label="Download preparation" />
          </>
        )}
        {job.state === 'ready' && job.url && (
          <p>
            Your file is ready.{' '}
            <a className="cpl-primary" href={job.url} download>
              Download
            </a>
          </p>
        )}
        {job.state === 'failed' && (
          <p role="alert">{job.error ?? 'We could not prepare this file.'}</p>
        )}
      </div>
    );
  } else {
    body = (
      <RadioGroup legend="File" value={choice} onValueChange={setChoice}>
        {rows.map((r) => (
          <Radio
            key={r.id}
            value={r.id}
            label={`${r.label}${r.kind === 'original' ? ' (original)' : ''} · ${formatBytes(r.size_bytes)}`}
            description={r.cached ? 'Ready to download' : 'Prepared when you ask, kept for 7 days'}
          />
        ))}
      </RadioGroup>
    );
  }

  const picking = !job && rows.length > 0 && !notPatron;
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={`Download ${title}`}
      description="A single file you can keep on this device."
      size="sm"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Close
          </Button>
          {picking && (
            <Button disabled={!choice} loading={start.isPending} onClick={() => start.mutate()}>
              Prepare download
            </Button>
          )}
        </>
      }
    >
      {body}
      {failure && <p role="alert">{failure}</p>}
    </Modal>
  );
}
