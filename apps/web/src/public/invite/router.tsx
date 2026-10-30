import { createRoute, type AnyRoute, type RouterHistory } from '@tanstack/react-router';
import { ServerInvite } from '../../routes/entrance/ServerInvite';
import { ServerTemplate } from '../../routes/entrance/ServerTemplate';
import { NoInvitation } from '../../routes/entrance/Static';
import { createSurfaceRouter } from '../surface';

/** The invite surface: `/invite/$code`, `/template/$code`, `/no-invitation`. */
export const createInviteRouter = (history?: RouterHistory) =>
  createSurfaceRouter((root, route) => {
    const invite: AnyRoute = createRoute({
      getParentRoute: () => root,
      path: '/invite/$code',
      component: function Invite() {
        const { code } = invite.useParams() as { code: string };
        return <ServerInvite code={code} />;
      },
    });
    const template: AnyRoute = createRoute({
      getParentRoute: () => root,
      path: '/template/$code',
      component: function Template() {
        const { code } = template.useParams() as { code: string };
        return <ServerTemplate code={code} />;
      },
    });
    return [invite, template, route('/no-invitation', NoInvitation)];
  }, history);
