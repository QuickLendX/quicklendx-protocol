import { describe, it, expect, vi, beforeEach } from 'vitest';
import { promises as fsPromises } from 'fs';
import { loadPolicy, getPolicyState, PolicyState } from '../src/lib/logging/policy';

vi.mock('fs', async () => {
  const actual = await vi.importActual<typeof import('fs')>('fs');
  return {
    ...actual,
    promises: {
      ...actual.promises,
      readFile: vi.fn(),
    },
  };
});

describe('policy.ts', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('loads policy successfully', async () => {
    const mockPolicy = {
      public: ['id'],
      private: ['email'],
      secret: ['password']
    };
    vi.mocked(fsPromises.readFile).mockResolvedValueOnce(JSON.stringify(mockPolicy));

    await loadPolicy();
    expect(getPolicyState()).toBe(PolicyState.LOADED);
  });

  it('retries and eventually fails', async () => {
    const error = new Error('read error');
    vi.mocked(fsPromises.readFile).mockRejectedValue(error);

    await expect(loadPolicy()).rejects.toThrow('read error');
    expect(getPolicyState()).toBe(PolicyState.ERROR);
  });
  
  it('sets state to PERMISSION_DENIED on EACCES', async () => {
    const error: any = new Error('permission denied');
    error.code = 'EACCES';
    vi.mocked(fsPromises.readFile).mockRejectedValueOnce(error);

    await expect(loadPolicy()).rejects.toThrow('permission denied');
    expect(getPolicyState()).toBe(PolicyState.PERMISSION_DENIED);
  });
});
