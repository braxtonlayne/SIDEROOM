// R-10 (D-362): after a successful prompt the single retry's permission refusal propagates and does not reopen the dialog.
import { ApiError } from '../../api/client';
import { registerReauthPrompt, withReauth } from '../reauth';

const refusal = (code?: number) =>
  new ApiError(403, 'refused', code === undefined ? null : { code, message: 'x' });

describe('withReauth', () => {
  it('prompts once, retries once, and lets a plain 403 through', async () => {
    let prompts = 0;
    const off = registerReauthPrompt(async () => {
      prompts += 1;
      return true;
    });
    let calls = 0;
    const call = async () => {
      calls += 1;
      throw calls === 1 ? refusal(900004) : refusal(50013);
    };
    await expect(withReauth(call)).rejects.toMatchObject({ status: 403, body: { code: 50013 } });
    expect(prompts).toBe(1);
    expect(calls).toBe(2);
    off();
  });
});
