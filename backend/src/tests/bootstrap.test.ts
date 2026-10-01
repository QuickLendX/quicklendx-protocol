import * as fs from "fs/promises";
import * as os from "os";
import * as path from "path";

jest.mock("fs/promises", () => {
  const actual = jest.requireActual<typeof fs>("fs/promises");
  return {
    ...actual,
    mkdir: jest.fn(actual.mkdir),
    stat: jest.fn(actual.stat),
  };
});

const realFs = jest.requireActual<typeof fs>("fs/promises");
const mkdir = jest.mocked(fs.mkdir);
const stat = jest.mocked(fs.stat);
const failure = (code: string) =>
  Object.assign(new Error("private /path secret"), { code });

describe("ensureRuntimeDirs failure boundaries", () => {
  let root: string;
  let ensureRuntimeDirs: () => Promise<void>;
  let log: jest.SpyInstance;
  let warn: jest.SpyInstance;

  beforeEach(async () => {
    root = await realFs.mkdtemp(path.join(os.tmpdir(), "qlx-bootstrap-"));
    mkdir.mockReset().mockImplementation(realFs.mkdir);
    stat.mockReset().mockImplementation(realFs.stat);
    log = jest.spyOn(console, "log").mockImplementation(() => {});
    warn = jest.spyOn(console, "warn").mockImplementation(() => {});
    const cwd = jest.spyOn(process, "cwd").mockReturnValue(root);
    try {
      jest.isolateModules(() => {
        ensureRuntimeDirs = require("../lib/bootstrap").ensureRuntimeDirs;
      });
    } finally {
      cwd.mockRestore();
    }
  });

  afterEach(async () => {
    jest.restoreAllMocks();
    await realFs.rm(root, { recursive: true, force: true });
  });

  it("creates both directories and returns void without changing the caller interface", async () => {
    await expect(ensureRuntimeDirs()).resolves.toBeUndefined();
    for (const name of [".data", ".hotfix-approvals"]) {
      expect((await realFs.stat(path.join(root, name))).isDirectory()).toBe(
        true,
      );
    }
    expect(mkdir.mock.calls).toEqual([
      [path.join(root, ".data"), { recursive: true }],
      [path.join(root, ".hotfix-approvals"), { recursive: true }],
    ]);
    expect(log).toHaveBeenCalledTimes(2);
    expect(warn).not.toHaveBeenCalled();
  });

  it("keeps existing database and approval bytes intact across duplicate calls", async () => {
    await ensureRuntimeDirs();
    const database = path.join(root, ".data", "database.sqlite");
    const approval = path.join(root, ".hotfix-approvals", "approval.json");
    await realFs.writeFile(database, Buffer.from([0, 1, 255, 42]));
    await realFs.writeFile(approval, "signed approval");
    await ensureRuntimeDirs();
    await ensureRuntimeDirs();
    expect(await realFs.readFile(database)).toEqual(
      Buffer.from([0, 1, 255, 42]),
    );
    expect(await realFs.readFile(approval, "utf8")).toBe("signed approval");
    expect(warn).not.toHaveBeenCalled();
  });

  it.each([".data", ".hotfix-approvals"])(
    "reports a blocking file at %s without replacing it or blocking the other directory",
    async (name) => {
      const blocked = path.join(root, name);
      await realFs.writeFile(blocked, "do not remove");
      await expect(ensureRuntimeDirs()).resolves.toBeUndefined();
      expect(await realFs.readFile(blocked, "utf8")).toBe("do not remove");
      const other = name === ".data" ? ".hotfix-approvals" : ".data";
      expect((await realFs.stat(path.join(root, other))).isDirectory()).toBe(
        true,
      );
      expect(warn).toHaveBeenCalledWith(
        `⚠️  Could not create ${name}/:`,
        "EEXIST",
      );
      expect(log).toHaveBeenCalledTimes(1);
    },
  );

  it.each(["EACCES", "EPERM", "ENOSPC", "EROFS", "ENOENT", "ENOTDIR"])(
    "recovers from %s on retry without rolling back the other directory",
    async (code) => {
      mkdir.mockRejectedValueOnce(failure(code));
      await expect(ensureRuntimeDirs()).resolves.toBeUndefined();
      await expect(realFs.stat(path.join(root, ".data"))).rejects.toMatchObject(
        { code: "ENOENT" },
      );
      const approval = path.join(root, ".hotfix-approvals", "keep.json");
      await realFs.writeFile(approval, "keep");
      expect(warn).toHaveBeenCalledWith("⚠️  Could not create .data/:", code);
      await ensureRuntimeDirs();
      expect((await realFs.stat(path.join(root, ".data"))).isDirectory()).toBe(
        true,
      );
      expect(await realFs.readFile(approval, "utf8")).toBe("keep");
    },
  );

  it("also retries a failure of the second directory while retaining the first", async () => {
    mkdir
      .mockImplementationOnce(realFs.mkdir)
      .mockRejectedValueOnce(failure("EACCES"));
    await ensureRuntimeDirs();
    const database = path.join(root, ".data", "keep.sqlite");
    await realFs.writeFile(database, "database");
    expect(warn).toHaveBeenCalledWith(
      "⚠️  Could not create .hotfix-approvals/:",
      "EACCES",
    );
    await ensureRuntimeDirs();
    expect(await realFs.readFile(database, "utf8")).toBe("database");
    expect(
      (await realFs.stat(path.join(root, ".hotfix-approvals"))).isDirectory(),
    ).toBe(true);
  });

  it("diagnoses two failures independently and never logs raw errors or paths", async () => {
    mkdir
      .mockRejectedValueOnce(failure("EACCES"))
      .mockRejectedValueOnce(failure("ENOSPC"));
    await expect(ensureRuntimeDirs()).resolves.toBeUndefined();
    expect(warn.mock.calls).toEqual([
      ["⚠️  Could not create .data/:", "EACCES"],
      ["⚠️  Could not create .hotfix-approvals/:", "ENOSPC"],
    ]);
    expect(log).not.toHaveBeenCalled();
    expect(JSON.stringify(warn.mock.calls)).not.toContain(root);
    expect(JSON.stringify(warn.mock.calls)).not.toContain("secret");
  });

  it.each([
    undefined,
    null,
    "private secret",
    new Error("private secret"),
    { code: 42 },
    { code: "secret\nTOKEN" },
  ])(
    "handles malformed rejection %# without skipping the second directory",
    async (error) => {
      mkdir.mockRejectedValueOnce(error);
      await expect(ensureRuntimeDirs()).resolves.toBeUndefined();
      expect(warn).toHaveBeenCalledWith(
        "⚠️  Could not create .data/:",
        "UNKNOWN",
      );
      expect(
        (await realFs.stat(path.join(root, ".hotfix-approvals"))).isDirectory(),
      ).toBe(true);
      expect(JSON.stringify(warn.mock.calls)).not.toContain("secret");
    },
  );

  it("accepts EEXIST only after confirming that a competing creator made a directory", async () => {
    await realFs.mkdir(path.join(root, ".data"));
    mkdir.mockRejectedValueOnce(failure("EEXIST"));
    await ensureRuntimeDirs();
    expect(stat).toHaveBeenCalledWith(path.join(root, ".data"));
    expect(log).toHaveBeenCalledTimes(2);
    expect(warn).not.toHaveBeenCalled();
  });

  it.each(["ENOENT", "EACCES"])(
    "reports %s when EEXIST verification races with removal or loses permission",
    async (code) => {
      mkdir.mockRejectedValueOnce(failure("EEXIST"));
      stat.mockRejectedValueOnce(failure(code));
      await ensureRuntimeDirs();
      expect(warn).toHaveBeenCalledWith("⚠️  Could not create .data/:", code);
      expect(log).toHaveBeenCalledTimes(1);
      await ensureRuntimeDirs();
      expect((await realFs.stat(path.join(root, ".data"))).isDirectory()).toBe(
        true,
      );
    },
  );

  it("remains idempotent under concurrent real filesystem calls", async () => {
    await Promise.all(Array.from({ length: 20 }, () => ensureRuntimeDirs()));
    expect((await realFs.stat(path.join(root, ".data"))).isDirectory()).toBe(
      true,
    );
    expect(
      (await realFs.stat(path.join(root, ".hotfix-approvals"))).isDirectory(),
    ).toBe(true);
    expect(warn).not.toHaveBeenCalled();
  });

  it("does not share a failed in-flight attempt with another caller", async () => {
    let rejectFirst!: (error: Error) => void;
    mkdir.mockImplementationOnce(
      () =>
        new Promise((_, reject) => {
          rejectFirst = reject;
        }),
    );
    const first = ensureRuntimeDirs();
    await ensureRuntimeDirs();
    rejectFirst(failure("EACCES"));
    await first;
    expect(warn).toHaveBeenCalledTimes(1);
    expect((await realFs.stat(path.join(root, ".data"))).isDirectory()).toBe(
      true,
    );
    expect(
      (await realFs.stat(path.join(root, ".hotfix-approvals"))).isDirectory(),
    ).toBe(true);
  });
});
