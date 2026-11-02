import { render, screen } from '@testing-library/react';
import { act } from 'react';
import { afterEach, describe, expect, it } from 'vitest';
import { activityBase } from '../../activities/api';
import { setClientFlags } from '../../moderation/reports';
import { TEST_DEPLOYMENT_FLAG, isTestDeployment, noteClientFlags, useTestDeployment } from '../flag';
import { TestDeploymentNotice } from '../TestDeploymentNotice';

const where = { protocol: 'https:', host: 'abc.trycloudflare.com' };

afterEach(() => useTestDeployment.setState({ on: false }));

describe('OPS-TDEP-001: the test deployment (same-host Activities)', () => {
  it('builds the Activity address for each origin mode', () => {
    expect(activityBase('42', where, false)).toBe('https://42.activities.abc.trycloudflare.com');
    expect(activityBase('42', where, true)).toBe('https://abc.trycloudflare.com/_activity/42');
    // The default follows the server's flag.
    expect(activityBase('42', where)).toBe('https://42.activities.abc.trycloudflare.com');
    noteClientFlags({ [TEST_DEPLOYMENT_FLAG]: true });
    expect(activityBase('42', where)).toBe('https://abc.trycloudflare.com/_activity/42');
  });

  it('keeps the mode once the server said so; a gateway flag update without it does not undo it', () => {
    expect(isTestDeployment()).toBe(false);
    setClientFlags({ 'moderation.reports': true });
    expect(isTestDeployment()).toBe(false);
    setClientFlags({ [TEST_DEPLOYMENT_FLAG]: true });
    expect(isTestDeployment()).toBe(true);
    setClientFlags({ 'moderation.reports': false });
    expect(isTestDeployment()).toBe(true);
    // Only `true` counts.
    useTestDeployment.setState({ on: false });
    noteClientFlags({ [TEST_DEPLOYMENT_FLAG]: false });
    expect(isTestDeployment()).toBe(false);
  });

  it('shows the notice only while the mode is on', () => {
    const { container } = render(<TestDeploymentNotice />);
    expect(container).toBeEmptyDOMElement();
    act(() => noteClientFlags({ [TEST_DEPLOYMENT_FLAG]: true }));
    const note = screen.getByTestId('test-deployment-notice');
    expect(note).toHaveAttribute('role', 'note');
    expect(note).toHaveTextContent('Test deployment');
  });
});
