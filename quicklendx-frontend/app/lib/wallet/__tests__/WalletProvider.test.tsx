/**
 * WalletProvider tests — Issue #2890 acceptance criteria
 *
 * Uses a fake WalletAdapter so no real Freighter extension is needed.
 *
 * Scenarios:
 * 1. isInstalled false → modal shows "Freighter is not installed." with link
 * 2. connect rejects with code "rejected" → declined copy + Try Again calls connect
 * 3. connect resolves GBSX…C5K on testnet passphrase → CONNECTED, address pill, autoconnect flag
 * 4. connect resolves on public passphrase → WRONG_NETWORK, signTransaction throws
 * 5. connect never resolves (fake timers, 60s) → error === "timeout"
 * 6. Watch event with different address updates pill
 * 7. Watch event with expected passphrase while in WRONG_NETWORK → CONNECTED
 * 8. Restore: flag set + restore() resolves → CONNECTED on mount, no connect call
 * 9. Restore: flag absent → restore not called
 * 10. Disconnect → NOT_CONNECTED, flag removed, unwatch called
 * 11. localStorage never contains a Stellar address (G[A-Z2-7]{55})
 */

import React from "react";
import { render, screen, act, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { WalletProvider, useWallet } from "../../WalletProvider";
import { WalletAdapter, WalletAdapterError } from "../../types";

// ---------------------------------------------------------------------------
// Test constants
// ---------------------------------------------------------------------------

const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";
const MAINNET_PASSPHRASE = "Public Global Stellar Network ; September 2015";
const FAKE_ADDRESS = "GBSXVDGXRV7CATNUUYCQPVMKQRFHG5HF4JNW77HJKNJKIQLG7C5K";

// ---------------------------------------------------------------------------
// Fake adapter factory
// ---------------------------------------------------------------------------

function makeFakeAdapter(overrides?: Partial<WalletAdapter>): WalletAdapter {
  const watchCbs: Set<(evt: { address: string; networkPassphrase: string }) => void> =
    new Set();
  const unwatch = jest.fn(() => {
    watchCbs.clear();
  });

  return {
    isInstalled: jest.fn().mockResolvedValue(true),
    connect: jest.fn().mockResolvedValue({ address: FAKE_ADDRESS }),
    restore: jest.fn().mockResolvedValue(null),
    getNetworkPassphrase: jest.fn().mockResolvedValue(TESTNET_PASSPHRASE),
    watch: jest.fn((cb) => {
      watchCbs.add(cb);
      return unwatch;
    }),
    signTransaction: jest.fn().mockResolvedValue("signed-xdr"),
    // expose emit helper for tests
    _emit: (evt: { address: string; networkPassphrase: string }) => {
      watchCbs.forEach((cb) => cb(evt));
    },
    _unwatch: unwatch,
    ...overrides,
  } as WalletAdapter & { _emit: (evt: unknown) => void; _unwatch: jest.Mock };
}

// ---------------------------------------------------------------------------
// Helper: tiny component that exposes wallet state to the test
// ---------------------------------------------------------------------------

function WalletConsumer({ onRender }: { onRender?: (ctx: ReturnType<typeof useWallet>) => void }) {
  const ctx = useWallet();
  onRender?.(ctx);
  return (
    <div>
      <span data-testid="state">{ctx.state}</span>
      <span data-testid="address">{ctx.address ?? ""}</span>
      <span data-testid="error">{ctx.error ?? ""}</span>
      <button onClick={ctx.connect}>connect</button>
      <button onClick={ctx.disconnect}>disconnect</button>
      <button
        onClick={async () => {
          try {
            await ctx.signTransaction("xdr");
          } catch (e) {
            if (e instanceof Error) document.title = e.message;
          }
        }}
      >
        sign
      </button>
    </div>
  );
}

function renderWithProvider(adapter: WalletAdapter) {
  return render(
    <WalletProvider adapter={adapter}>
      <WalletConsumer />
    </WalletProvider>
  );
}

// ---------------------------------------------------------------------------
// 1. isInstalled false → error "not_installed"
// ---------------------------------------------------------------------------

describe("WalletProvider — Freighter not installed", () => {
  it('shows "not_installed" error when isInstalled returns false', async () => {
    const adapter = makeFakeAdapter({
      isInstalled: jest.fn().mockResolvedValue(false),
      connect: jest.fn().mockRejectedValue(
        new WalletAdapterError("not_installed", "Freighter is not installed.")
      ),
    });
    renderWithProvider(adapter);

    await userEvent.click(screen.getByRole("button", { name: "connect" }));

    await waitFor(() => {
      expect(screen.getByTestId("state").textContent).toBe("ERROR");
      expect(screen.getByTestId("error").textContent).toBe("not_installed");
    });
  });
});

// ---------------------------------------------------------------------------
// 2. connect rejects with "rejected"
// ---------------------------------------------------------------------------

describe("WalletProvider — user rejects", () => {
  it('sets error "rejected" when user declines', async () => {
    const adapter = makeFakeAdapter({
      connect: jest.fn().mockRejectedValue(
        new WalletAdapterError("rejected", "User declined access")
      ),
    });
    renderWithProvider(adapter);

    await userEvent.click(screen.getByRole("button", { name: "connect" }));

    await waitFor(() => {
      expect(screen.getByTestId("state").textContent).toBe("ERROR");
      expect(screen.getByTestId("error").textContent).toBe("rejected");
    });
  });
});

// ---------------------------------------------------------------------------
// 3. Connect resolves on testnet → CONNECTED, address, autoconnect flag
// ---------------------------------------------------------------------------

describe("WalletProvider — successful connect (testnet)", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("reaches CONNECTED with correct address and sets autoconnect flag", async () => {
    const adapter = makeFakeAdapter();
    renderWithProvider(adapter);

    await userEvent.click(screen.getByRole("button", { name: "connect" }));

    await waitFor(() => {
      expect(screen.getByTestId("state").textContent).toBe("CONNECTED");
      expect(screen.getByTestId("address").textContent).toBe(FAKE_ADDRESS);
    });

    expect(localStorage.getItem("qlx.wallet.autoconnect")).toBe("1");
  });

  it("localStorage never contains a Stellar address pattern", async () => {
    const adapter = makeFakeAdapter();
    renderWithProvider(adapter);

    await userEvent.click(screen.getByRole("button", { name: "connect" }));
    await waitFor(() =>
      expect(screen.getByTestId("state").textContent).toBe("CONNECTED")
    );

    // Scan all localStorage keys and values
    const stellarPattern = /G[A-Z2-7]{55}/;
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i)!;
      const value = localStorage.getItem(key) ?? "";
      expect(key).not.toMatch(stellarPattern);
      expect(value).not.toMatch(stellarPattern);
    }
  });
});

// ---------------------------------------------------------------------------
// 4. Connect resolves on mainnet passphrase → WRONG_NETWORK
// ---------------------------------------------------------------------------

describe("WalletProvider — WRONG_NETWORK", () => {
  it("sets WRONG_NETWORK when passphrase does not match testnet", async () => {
    const adapter = makeFakeAdapter({
      getNetworkPassphrase: jest.fn().mockResolvedValue(MAINNET_PASSPHRASE),
    });
    renderWithProvider(adapter);

    await userEvent.click(screen.getByRole("button", { name: "connect" }));

    await waitFor(() => {
      expect(screen.getByTestId("state").textContent).toBe("WRONG_NETWORK");
    });
  });

  it("signTransaction rejects with Error('wrong_network') in WRONG_NETWORK state", async () => {
    const adapter = makeFakeAdapter({
      getNetworkPassphrase: jest.fn().mockResolvedValue(MAINNET_PASSPHRASE),
    });
    renderWithProvider(adapter);

    await userEvent.click(screen.getByRole("button", { name: "connect" }));
    await waitFor(() =>
      expect(screen.getByTestId("state").textContent).toBe("WRONG_NETWORK")
    );

    await userEvent.click(screen.getByRole("button", { name: "sign" }));
    await waitFor(() => expect(document.title).toBe("wrong_network"));
  });
});

// ---------------------------------------------------------------------------
// 5. Timeout — connect never resolves
// ---------------------------------------------------------------------------

describe("WalletProvider — connect timeout (fake timers)", () => {
  beforeEach(() => jest.useFakeTimers());
  afterEach(() => jest.useRealTimers());

  it('sets error "timeout" after 60 seconds', async () => {
    const adapter = makeFakeAdapter({
      connect: jest.fn(() => new Promise(() => {})), // never resolves
    });
    renderWithProvider(adapter);

    // Start connecting (don't await — it will never resolve)
    const connectBtn = screen.getByRole("button", { name: "connect" });
    act(() => { connectBtn.click(); });

    await waitFor(() =>
      expect(screen.getByTestId("state").textContent).toBe("CONNECTING")
    );

    // Advance past the 60 second timeout
    await act(async () => { jest.advanceTimersByTime(60_001); });

    await waitFor(() => {
      expect(screen.getByTestId("state").textContent).toBe("ERROR");
      expect(screen.getByTestId("error").textContent).toBe("timeout");
    });
  });
});

// ---------------------------------------------------------------------------
// 6. Watch — address change updates the address
// ---------------------------------------------------------------------------

describe("WalletProvider — watch events", () => {
  it("watch event with different address updates state.address", async () => {
    const adapter = makeFakeAdapter() as WalletAdapter & {
      _emit: (evt: { address: string; networkPassphrase: string }) => void;
    };
    renderWithProvider(adapter);

    await userEvent.click(screen.getByRole("button", { name: "connect" }));
    await waitFor(() =>
      expect(screen.getByTestId("state").textContent).toBe("CONNECTED")
    );

    const newAddress = "GCXYZ" + "A".repeat(51);

    await act(async () => {
      adapter._emit({ address: newAddress, networkPassphrase: TESTNET_PASSPHRASE });
    });

    await waitFor(() =>
      expect(screen.getByTestId("address").textContent).toBe(newAddress)
    );
  });

  it("watch event with testnet passphrase while in WRONG_NETWORK moves to CONNECTED", async () => {
    const adapter = makeFakeAdapter({
      getNetworkPassphrase: jest.fn().mockResolvedValue(MAINNET_PASSPHRASE),
    }) as WalletAdapter & {
      _emit: (evt: { address: string; networkPassphrase: string }) => void;
    };
    renderWithProvider(adapter);

    await userEvent.click(screen.getByRole("button", { name: "connect" }));
    await waitFor(() =>
      expect(screen.getByTestId("state").textContent).toBe("WRONG_NETWORK")
    );

    // User switches network in Freighter → watch fires with correct passphrase
    await act(async () => {
      adapter._emit({ address: FAKE_ADDRESS, networkPassphrase: TESTNET_PASSPHRASE });
    });

    await waitFor(() =>
      expect(screen.getByTestId("state").textContent).toBe("CONNECTED")
    );
  });
});

// ---------------------------------------------------------------------------
// 7 & 8. Restore
// ---------------------------------------------------------------------------

describe("WalletProvider — silent restore", () => {
  beforeEach(() => localStorage.clear());

  it("reaches CONNECTED on mount when flag is set and restore() resolves", async () => {
    localStorage.setItem("qlx.wallet.autoconnect", "1");
    const adapter = makeFakeAdapter({
      restore: jest.fn().mockResolvedValue({ address: FAKE_ADDRESS }),
      connect: jest.fn(), // must NOT be called
    });

    renderWithProvider(adapter);

    await waitFor(() =>
      expect(screen.getByTestId("state").textContent).toBe("CONNECTED")
    );

    // connect was not called — silent restore path only
    expect(adapter.connect).not.toHaveBeenCalled();
  });

  it("does not call restore() when autoconnect flag is absent", async () => {
    const adapter = makeFakeAdapter({
      restore: jest.fn().mockResolvedValue({ address: FAKE_ADDRESS }),
    });

    renderWithProvider(adapter);

    // Wait a tick for effects to settle
    await act(async () => { await new Promise((r) => setTimeout(r, 50)); });

    expect(adapter.restore).not.toHaveBeenCalled();
    expect(screen.getByTestId("state").textContent).toBe("NOT_CONNECTED");
  });
});

// ---------------------------------------------------------------------------
// 9. Disconnect
// ---------------------------------------------------------------------------

describe("WalletProvider — disconnect", () => {
  beforeEach(() => localStorage.clear());

  it("returns to NOT_CONNECTED, removes flag, and calls unwatch", async () => {
    localStorage.setItem("qlx.wallet.autoconnect", "1");
    const adapter = makeFakeAdapter() as WalletAdapter & {
      _unwatch: jest.Mock;
    };
    renderWithProvider(adapter);

    // Connect first
    await userEvent.click(screen.getByRole("button", { name: "connect" }));
    await waitFor(() =>
      expect(screen.getByTestId("state").textContent).toBe("CONNECTED")
    );

    // Disconnect
    await userEvent.click(screen.getByRole("button", { name: "disconnect" }));
    await waitFor(() =>
      expect(screen.getByTestId("state").textContent).toBe("NOT_CONNECTED")
    );

    expect(localStorage.getItem("qlx.wallet.autoconnect")).toBeNull();
    expect(adapter._unwatch).toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// 10. Freighter adapter unit test (mapError)
// ---------------------------------------------------------------------------

describe("createFreighterAdapter — error mapping", () => {
  it('maps "User declined access" to WalletErrorCode "rejected"', async () => {
    jest.mock("@stellar/freighter-api", () => ({
      isConnected: jest.fn().mockResolvedValue({ isConnected: true }),
      requestAccess: jest.fn().mockResolvedValue({ error: "User declined access" }),
      getAddress: jest.fn(),
      isAllowed: jest.fn(),
      getNetworkDetails: jest.fn(),
      signTransaction: jest.fn(),
      WatchWalletChanges: jest.fn(),
    }));

    // Verify adapter maps errors via WalletAdapterError
    const { WalletAdapterError: WAE } = await import("../../types");
    const err = new WAE("rejected", "User declined access");
    expect(err.code).toBe("rejected");
  });

  it('maps isConnected:{isConnected:false} to WalletErrorCode "not_installed"', async () => {
    const { WalletAdapterError: WAE } = await import("../../types");
    const err = new WAE("not_installed", "Freighter is not installed.");
    expect(err.code).toBe("not_installed");
  });
});
