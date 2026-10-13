import { Badge, Button, Notice, Switch } from '@sideroom/ui';
import { useState } from 'react';
import * as api from '../api';
import { Async, ReasonDialog, SectionHeader, useResource } from '../ui';

export function FeaturesSection() {
  const res = useResource('admin:features', api.listFeatures);
  const [target, setTarget] = useState<{ f: api.FeatureView; enabled: boolean | 'reset' } | null>(
    null,
  );
  const [notice, setNotice] = useState<string | null>(null);

  return (
    <>
      <SectionHeader
        title="Feature modules"
        description="Platform-wide kill switches. A disabled feature is hidden from everyone, whatever servers or members have set."
      />
      {notice && (
        <Notice variant="success" role="status" className="ss-status">
          {notice}
        </Notice>
      )}
      <Async res={res} label="Loading features">
        {(list) =>
          list.length === 0 ? (
            <p className="ss-na">No feature modules are registered yet.</p>
          ) : (
            <ul className="ss-list" aria-label="Feature modules">
              {list.map((f) => (
                <li key={f.id} className="ss-row">
                  <div className="ss-grow">
                    <div className="ss-name">
                      {f.name} <Badge>{f.section}</Badge>{' '}
                      {f.enabled !== f.default_enabled && (
                        <Badge variant="warning">overridden</Badge>
                      )}
                    </div>
                    <div className="ss-meta">{f.description}</div>
                  </div>
                  <Switch
                    label={`${f.name} enabled`}
                    checked={f.enabled}
                    onChange={(e) => setTarget({ f, enabled: e.target.checked })}
                  />
                  {f.enabled !== f.default_enabled && (
                    <Button
                      size="sm"
                      variant="ghost"
                      aria-label={`Reset ${f.name} to its default`}
                      onClick={() => setTarget({ f, enabled: 'reset' })}
                    >
                      Reset
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          )
        }
      </Async>
      <ReasonDialog
        open={!!target}
        title={
          target?.enabled === 'reset'
            ? `Reset ${target.f.name}`
            : `${target?.enabled ? 'Enable' : 'Disable'} ${target?.f.name ?? ''}`
        }
        description={
          target?.enabled === false
            ? 'People lose access to this feature immediately.'
            : target?.enabled === 'reset'
              ? `Returns to the default (${target.f.default_enabled ? 'on' : 'off'}).`
              : undefined
        }
        confirmLabel={
          target?.enabled === 'reset' ? 'Reset' : target?.enabled ? 'Enable' : 'Disable'
        }
        danger={target?.enabled === false}
        reasonOptional
        onCancel={() => setTarget(null)}
        onConfirm={async (reason) => {
          if (!target) return;
          await api.patchFeature(target.f.id, {
            ...(target.enabled === 'reset' ? { reset_enabled: true } : { enabled: target.enabled }),
            ...(reason ? { reason } : {}),
          });
          setNotice(`${target.f.name} updated.`);
          setTarget(null);
          await res.reload();
        }}
      />
    </>
  );
}
