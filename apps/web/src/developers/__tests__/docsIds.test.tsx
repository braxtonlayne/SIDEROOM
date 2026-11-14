// SEC-AUD-190 (D-672 area): two developer-docs pages once shared the id `gateway`, and DOC_BY_ID keeps the later
// one, so the Gateway guide could not be opened. Ids are unique now and the guide links to the reference.
import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { DOC_BY_ID, DOC_PAGES } from '../docsContent';
import { openAt, signedIn } from './helpers';

beforeEach(signedIn);

describe('developer docs page ids', () => {
  it('every_doc_page_id_is_unique', () => {
    const ids = DOC_PAGES.map((p) => p.id);
    expect(ids.filter((id, i) => ids.indexOf(id) !== i)).toEqual([]);
    expect(DOC_BY_ID.size).toBe(DOC_PAGES.length);
  });

  it('opens the Gateway guide at its own URL and links to the reference', async () => {
    const { router } = openAt('/developers/docs/gateway');
    expect(await screen.findByRole('heading', { name: 'Gateway', level: 1 })).toBeInTheDocument();
    expect(screen.getByText(/send RESUME with your session id/)).toBeInTheDocument();
    await userEvent
      .setup()
      .click(
        screen.getByRole('link', { name: /opcode and event tables are in the Gateway reference/ }),
      );
    expect(router.state.location.pathname).toBe('/developers/docs/gateway-events');
    expect(await screen.findByRole('table', { name: 'Gateway opcodes' })).toBeInTheDocument();
  });
});
