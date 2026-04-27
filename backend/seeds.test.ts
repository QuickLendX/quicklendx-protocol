import { seedScript } from './seed';

jest.spyOn(console, 'log').mockImplementation(() => {});
jest.spyOn(console, 'error').mockImplementation(() => {});

describe('Seed Script Smoke Tests', () => {
  const originalEnv = process.env.NODE_ENV;

  afterEach(() => {
    process.env.NODE_ENV = originalEnv;
    jest.clearAllMocks();
  });

  it('should run successfully in development environment', async () => {
    process.env.NODE_ENV = 'development';
    await expect(seedScript()).resolves.not.toThrow();
    expect(console.log).toHaveBeenCalledWith(expect.stringContaining('Starting database seed...'));
    expect(console.log).toHaveBeenCalledWith(expect.stringContaining('Seed completed successfully'));
  });

  it('should fail and exit if run in production', async () => {
    process.env.NODE_ENV = 'production';
    const mockExit = jest.spyOn(process, 'exit').mockImplementation((code?: string | number) => {
        throw new Error('process.exit: ' + code);
    });

    await expect(seedScript()).rejects.toThrow('process.exit: 1');
    expect(console.error).toHaveBeenCalledWith(expect.stringContaining('cannot be run in production'));
    mockExit.mockRestore();
  });

  it('should output log messages for clearing and creating data', async () => {
    process.env.NODE_ENV = 'development';
    await seedScript();
    expect(console.log).toHaveBeenCalledWith(expect.stringContaining('Clearing existing data'));
    expect(console.log).toHaveBeenCalledWith(expect.stringContaining('Creating sample users'));
  });
});