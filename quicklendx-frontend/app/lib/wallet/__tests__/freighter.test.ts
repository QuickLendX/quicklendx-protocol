/**
 * Unit tests for the Freighter wallet adapter (issue #3164).
 *
 * @stellar/freighter-api is mocked, so no browser extension is needed.
 * Every WalletErrorCode in ../types is produced by at least one test:
 *   not_installed - isConnected reports false; requestAccess reports "not connected"
 *   rejected      - requestAccess / getAddress / signTransaction report a user decline
 *   timeout       - requestAccess never settles (jest fake timers)
 *   unknown       - unrecognised SDK error text, missing address, getNetworkDetails error
 */

import * as freighterApi from "@stellar/freighter-api";
import { createFreighterAdapter } from "../freighter";
import { WalletAdapterError } from "../types";

jest.mock("@stellar/freighter-api", () => ({
  isConnected: jest.fn(),
  isAllowed: jest.fn(),
  requestAccess: jest.fn(),
  getAddress: jest.fn(),
  getNetworkDetails: jest.fn(),
  signTransaction: jest.fn(),
  WatchWalletChanges: jest.fn(),
}));

const sdk = freighterApi as unknown as Record<string, jest.Mock>;

const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";
const PUBLIC_PASSPHRASE = "Public Global Stellar Network ; September 2015";
const FAKE_ADDRESS = "GBSXVDGXRV7CATNUUYCQPVMKQRFHG5HF4JNW77HJKNJKIQLG7C5K";
// Mirrors CONNECT_TIMEOUT_MS in freighter.ts (not exported).
const CONNECT_TIMEOUT_MS = 60_000;

/** Resolves with the error a promise rejects with; fails if it resolves. */
async function captureError(promise: Promise<unknown>): Promise<WalletAdapterError> {
  try {
    await promise;
  } catch (e) {
    return e as WalletAdapterError;
  }
  throw new Error("expected the promise to reject");
}

/** Lets the real event loop run pending timers and dynamic-import callbacks. */
const flushTimers = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  jest.resetAllMocks();
  // Happy-path defaults; each test overrides only the call it exercises.
  sdk.isConnected.mockResolvedValue({ isConnected: true });
  sdk.isAllowed.mockResolvedValue({ isAllowed: false });
  sdk.requestAccess.mockResolvedValue({ address: FAKE_ADDRESS });
  sdk.getAddress.mockResolvedValue({ address: FAKE_ADDRESS });
  sdk.getNetworkDetails.mockResolvedValue({
    network: "TESTNET",
    networkPassphrase: TESTNET_PASSPHRASE,
  });
  sdk.signTransaction.mockResolvedValue({ signedTxXdr: "SIGNED_XDR" });
});

afterEach(() => {
  jest.useRealTimers();
});

// ---------------------------------------------------------------------------
// isInstalled
// ---------------------------------------------------------------------------

describe("isInstalled", () => {
  it("returns true when Freighter reports isConnected: true", async () => {
    await expect(createFreighterAdapter().isInstalled()).resolves.toBe(true);
  });

  it("returns false when Freighter reports isConnected: false", async () => {
    sdk.isConnected.mockResolvedValue({ isConnected: false });
    await expect(createFreighterAdapter().isInstalled()).resolves.toBe(false);
  });

  it("never throws: resolves false when the SDK call rejects", async () => {
    sdk.isConnected.mockRejectedValue(new Error("extension unavailable"));
    await expect(createFreighterAdapter().isInstalled()).resolves.toBe(false);
  });

  it("never throws: resolves false when the SDK call throws synchronously", async () => {
    sdk.isConnected.mockImplementation(() => {
      throw new Error("window is not defined");
    });
    await expect(createFreighterAdapter().isInstalled()).resolves.toBe(false);
  });
});

// ---------------------------------------------------------------------------
// connect
// ---------------------------------------------------------------------------

describe("connect", () => {
  it("resolves with the address once access is granted", async () => {
    const adapter = createFreighterAdapter();
    await expect(adapter.connect()).resolves.toEqual({ address: FAKE_ADDRESS });
    expect(sdk.requestAccess).toHaveBeenCalledTimes(1);
    expect(sdk.getAddress).toHaveBeenCalledTimes(1);
  });

  it("maps isConnected: false to not_installed without prompting for access", async () => {
    sdk.isConnected.mockResolvedValue({ isConnected: false });

    const err = await captureError(createFreighterAdapter().connect());

    expect(err).toBeInstanceOf(WalletAdapterError);
    expect(err.code).toBe("not_installed");
    expect(err.message).toBe("Freighter is not installed.");
    expect(sdk.requestAccess).not.toHaveBeenCalled();
  });

  it.each([
    ["User declined access"],
    ["User rejected the request"],
    ["Request was declined by the user"],
  ])("maps requestAccess error %j to rejected", async (sdkError) => {
    sdk.requestAccess.mockResolvedValue({ error: sdkError });

    const err = await captureError(createFreighterAdapter().connect());

    expect(err.code).toBe("rejected");
    expect(err.message).toBe(sdkError);
    expect(sdk.getAddress).not.toHaveBeenCalled();
  });

  it("maps a requestAccess 'could not connect' error to not_installed", async () => {
    sdk.requestAccess.mockResolvedValue({ error: "Could not connect to Freighter" });

    const err = await captureError(createFreighterAdapter().connect());

    expect(err.code).toBe("not_installed");
    expect(err.message).toBe("Could not connect to Freighter");
  });

  it("maps an unrecognised requestAccess error to unknown", async () => {
    sdk.requestAccess.mockResolvedValue({ error: "Something unexpected happened" });

    const err = await captureError(createFreighterAdapter().connect());

    expect(err.code).toBe("unknown");
    expect(err.message).toBe("Something unexpected happened");
  });

  it("maps a getAddress error through the same mapping (unknown for a locked wallet)", async () => {
    sdk.getAddress.mockResolvedValue({ error: "Wallet is locked" });

    const err = await captureError(createFreighterAdapter().connect());

    expect(err.code).toBe("unknown");
    expect(err.message).toBe("Wallet is locked");
  });

  it("rejects with unknown when Freighter returns an empty address", async () => {
    sdk.getAddress.mockResolvedValue({ address: "" });

    const err = await captureError(createFreighterAdapter().connect());

    expect(err.code).toBe("unknown");
    expect(err.message).toBe("No address returned.");
  });

  it("times out after 60s when the access prompt never settles", async () => {
    jest.useFakeTimers();
    sdk.requestAccess.mockReturnValue(new Promise(() => undefined));
    const adapter = createFreighterAdapter();

    let settled = false;
    const outcome = captureError(adapter.connect()).then((e) => {
      settled = true;
      return e;
    });

    // Let the adapter reach the pending requestAccess call.
    for (let i = 0; i < 20; i++) {
      await Promise.resolve();
    }
    expect(sdk.requestAccess).toHaveBeenCalledTimes(1);

    jest.advanceTimersByTime(CONNECT_TIMEOUT_MS - 1);
    expect(settled).toBe(false);

    jest.advanceTimersByTime(1);
    const err = await outcome;
    expect(err).toBeInstanceOf(WalletAdapterError);
    expect(err.code).toBe("timeout");
  });
});

// ---------------------------------------------------------------------------
// restore
// ---------------------------------------------------------------------------

describe("restore", () => {
  it("returns the address when the user previously granted access", async () => {
    sdk.isAllowed.mockResolvedValue({ isAllowed: true });
    await expect(createFreighterAdapter().restore()).resolves.toEqual({
      address: FAKE_ADDRESS,
    });
  });

  it("returns null without reading the address when access was never granted", async () => {
    await expect(createFreighterAdapter().restore()).resolves.toBeNull();
    expect(sdk.getAddress).not.toHaveBeenCalled();
  });

  it("returns null when getAddress reports an error", async () => {
    sdk.isAllowed.mockResolvedValue({ isAllowed: true });
    sdk.getAddress.mockResolvedValue({ error: "Wallet is locked" });
    await expect(createFreighterAdapter().restore()).resolves.toBeNull();
  });

  it("returns null instead of throwing when the SDK call fails", async () => {
    sdk.isAllowed.mockRejectedValue(new Error("extension unavailable"));
    await expect(createFreighterAdapter().restore()).resolves.toBeNull();
  });
});

// ---------------------------------------------------------------------------
// getNetworkPassphrase (network mismatch reporting)
// ---------------------------------------------------------------------------

describe("getNetworkPassphrase", () => {
  it("returns the passphrase of the network the wallet is on", async () => {
    await expect(createFreighterAdapter().getNetworkPassphrase()).resolves.toBe(
      TESTNET_PASSPHRASE
    );
  });

  it("reports a wallet on another network by returning that network's passphrase", async () => {
    // The adapter does not compare against an expected network. It surfaces the
    // wallet's passphrase so the caller can detect a mismatch.
    sdk.getNetworkDetails.mockResolvedValue({
      network: "PUBLIC",
      networkPassphrase: PUBLIC_PASSPHRASE,
    });

    const passphrase = await createFreighterAdapter().getNetworkPassphrase();

    expect(passphrase).toBe(PUBLIC_PASSPHRASE);
    expect(passphrase).not.toBe(TESTNET_PASSPHRASE);
  });

  it("maps a getNetworkDetails error to unknown", async () => {
    sdk.getNetworkDetails.mockResolvedValue({ error: "Wallet is locked" });

    const err = await captureError(createFreighterAdapter().getNetworkPassphrase());

    expect(err).toBeInstanceOf(WalletAdapterError);
    expect(err.code).toBe("unknown");
    expect(err.message).toBe("Wallet is locked");
  });
});

// ---------------------------------------------------------------------------
// watch
// ---------------------------------------------------------------------------

describe("watch", () => {
  function stubWatcher() {
    const watcher = { watch: jest.fn(), stop: jest.fn() };
    sdk.WatchWalletChanges.mockImplementation(() => watcher);
    return watcher;
  }

  it("polls every 3s and forwards a network change with the new passphrase", async () => {
    const watcher = stubWatcher();
    const cb = jest.fn();

    createFreighterAdapter().watch(cb);
    await flushTimers();

    expect(sdk.WatchWalletChanges).toHaveBeenCalledWith(3000);
    const onWalletChange = watcher.watch.mock.calls[0][0] as (evt: unknown) => void;
    const evt = { address: FAKE_ADDRESS, networkPassphrase: PUBLIC_PASSPHRASE };
    onWalletChange(evt);
    expect(cb).toHaveBeenCalledWith(evt);
  });

  it("stops the watcher when the returned unsubscribe function is called", async () => {
    const watcher = stubWatcher();
    const unsubscribe = createFreighterAdapter().watch(jest.fn());
    await flushTimers();

    unsubscribe();

    expect(watcher.stop).toHaveBeenCalledTimes(1);
  });

  it("never starts a watcher when unsubscribed before the SDK loads", async () => {
    stubWatcher();
    const unsubscribe = createFreighterAdapter().watch(jest.fn());

    unsubscribe();
    await flushTimers();

    expect(sdk.WatchWalletChanges).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// signTransaction
// ---------------------------------------------------------------------------

describe("signTransaction", () => {
  const opts = { networkPassphrase: TESTNET_PASSPHRASE, address: FAKE_ADDRESS };

  it("returns the signed XDR and passes the passphrase and address to Freighter", async () => {
    await expect(
      createFreighterAdapter().signTransaction("UNSIGNED_XDR", opts)
    ).resolves.toBe("SIGNED_XDR");
    expect(sdk.signTransaction).toHaveBeenCalledWith("UNSIGNED_XDR", opts);
  });

  it("returns a bare string result unchanged when Freighter does not wrap it", async () => {
    sdk.signTransaction.mockResolvedValue("BARE_XDR");
    await expect(
      createFreighterAdapter().signTransaction("UNSIGNED_XDR", opts)
    ).resolves.toBe("BARE_XDR");
  });

  it("maps a user decline to rejected", async () => {
    sdk.signTransaction.mockResolvedValue({ error: "User declined to sign" });

    const err = await captureError(
      createFreighterAdapter().signTransaction("UNSIGNED_XDR", opts)
    );

    expect(err).toBeInstanceOf(WalletAdapterError);
    expect(err.code).toBe("rejected");
    expect(err.message).toBe("User declined to sign");
  });

  it("surfaces a wallet network-mismatch error text with code unknown", async () => {
    // The adapter has no dedicated network-mismatch code, so this is unknown.
    sdk.signTransaction.mockResolvedValue({
      error: "Network mismatch: wallet is on PUBLIC",
    });

    const err = await captureError(
      createFreighterAdapter().signTransaction("UNSIGNED_XDR", opts)
    );

    expect(err.code).toBe("unknown");
    expect(err.message).toBe("Network mismatch: wallet is on PUBLIC");
  });
});
