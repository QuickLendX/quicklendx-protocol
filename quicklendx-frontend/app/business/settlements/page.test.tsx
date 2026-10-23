import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type React from "react";
import SettlementsPage from "./page";
import { listSettlements, type Settlement } from "./_lib/data";
import { useWallet } from "@/app/lib/wallet/WalletProvider";

jest.mock("next/navigation", () => ({
  useRouter: () => ({ replace: mockReplace }),
  usePathname: () => "/business/settlements",
  useSearchParams: () => new URLSearchParams(mockSearch),
}));

jest.mock("next/link", () => {
  const React = require("react");
  return {
    __esModule: true,
    default: ({
      href,
      children,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { href: string }) =>
      React.createElement("a", { href, ...props }, children),
  };
});

jest.mock("./_lib/data", () => ({
  listSettlements: jest.fn(),
}));

jest.mock("@/app/lib/wallet/WalletProvider", () => ({
  useWallet: jest.fn(),
  WalletConnectControl: () =>
    require("react").createElement("button", null, "Connect Wallet"),
}));

const mockReplace = jest.fn((url: string) => {
  mockSearch = url.split("?")[1] ?? "";
});
let mockSearch = "";
const mockListSettlements = jest.mocked(listSettlements);
const mockUseWallet = jest.mocked(useWallet);
const connectedAddress = "GCONNECTEDWALLET";

function makeSettlement(
  index: number,
  overrides: Partial<Settlement> = {}
): Settlement {
  return {
    id: `settlement-${index}`,
    invoice_id: `0x1234567890abcdef${index.toString().padStart(4, "0")}`,
    amount: String(index + 1),
    payer: index % 2 ? connectedAddress : `GOUTSIDER${index}`,
    recipient: index % 2 ? `GOUTSIDER${index}` : connectedAddress,
    timestamp: Math.floor(Date.now() / 1000) - index * 60,
    status: index % 2 ? "Paid" : "Pending",
    ...overrides,
  };
}

function response(data: Settlement[], indexLagSeconds = 10) {
  return { data, freshness: { indexLagSeconds } };
}

async function renderConnected(rows: Settlement[], lag = 10) {
  mockUseWallet.mockReturnValue({
    state: "connected",
    address: connectedAddress,
  });
  mockListSettlements.mockResolvedValue(response(rows, lag));
  const rendered = render(<SettlementsPage />);
  await waitFor(() => expect(mockListSettlements).toHaveBeenCalledTimes(1));
  return rendered;
}

beforeEach(() => {
  mockSearch = "";
  mockReplace.mockClear();
  mockListSettlements.mockReset();
  mockUseWallet.mockReturnValue({
    state: "connected",
    address: connectedAddress,
  });
});

describe("Settlement receipts page", () => {
  it("shows 20 rows, loads more in pages of 20, and scopes results to the wallet", async () => {
    await renderConnected([
      ...Array.from({ length: 45 }, (_, index) => makeSettlement(index)),
      ...Array.from({ length: 5 }, (_, index) =>
        makeSettlement(100 + index, {
          payer: `GOTHER${index}`,
          recipient: `GOTHERRECIPIENT${index}`,
        })
      ),
    ]);

    await waitFor(() =>
      expect(screen.getAllByTestId("settlement-row")).toHaveLength(20)
    );
    expect(screen.getByRole("button", { name: "Load more" })).toBeTruthy();

    await userEvent.click(screen.getByRole("button", { name: "Load more" }));
    expect(screen.getAllByTestId("settlement-row")).toHaveLength(40);
    await userEvent.click(screen.getByRole("button", { name: "Load more" }));
    expect(screen.getAllByTestId("settlement-row")).toHaveLength(45);
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();

    for (const row of screen.getAllByTestId("settlement-row")) {
      expect(row.textContent).not.toContain("GOTHER");
      expect(row.textContent).not.toContain("GOUTSIDER");
    }
  });

  it("filters by status and mirrors the selected status in the URL", async () => {
    await renderConnected(
      Array.from({ length: 6 }, (_, index) => makeSettlement(index))
    );

    await userEvent.selectOptions(screen.getByLabelText("Status"), "Paid");
    await waitFor(() =>
      expect(screen.getAllByTestId("settlement-row")).toHaveLength(3)
    );
    expect(mockReplace).toHaveBeenCalledWith(
      "/business/settlements?status=Paid",
      { scroll: false }
    );
    for (const row of screen.getAllByTestId("settlement-row")) {
      expect(row.textContent).toContain("Paid");
      expect(row.textContent).not.toContain("Pending");
    }
  });

  it("sorts large amounts using BigInt rather than Number", async () => {
    await renderConnected(
      [
        makeSettlement(1, { amount: "5" }),
        makeSettlement(2, { amount: "900000000000000000000" }),
      ],
      10
    );

    await userEvent.selectOptions(screen.getByLabelText("Date range"), "all");
    await userEvent.selectOptions(screen.getByLabelText("Sort"), "amount_desc");
    const rows = screen.getAllByTestId("settlement-row");
    expect(rows[0].textContent).toContain("900000000000000000000");
    expect(rows[1].textContent).toContain("5");
  });

  it("sums values above Number safe integer precision exactly", async () => {
    await renderConnected([
      makeSettlement(1, { amount: "9007199254740993" }),
      makeSettlement(2, { amount: "1" }),
    ]);

    expect(
      await screen.findByText("Total this period: 9007199254740994 units")
    ).toBeTruthy();
  });

  it("renders five accessible loading skeleton rows", async () => {
    let resolveFetch!: (value: ReturnType<typeof response>) => void;
    mockUseWallet.mockReturnValue({
      state: "connected",
      address: connectedAddress,
    });
    mockListSettlements.mockReturnValue(
      new Promise((resolve) => {
        resolveFetch = resolve;
      })
    );
    render(<SettlementsPage />);

    expect((await screen.findByRole("table")).getAttribute("aria-busy")).toBe(
      "true"
    );
    expect(screen.getAllByTestId("settlement-skeleton-row")).toHaveLength(5);
    await act(async () => resolveFetch(response([])));
  });

  it("shows the empty-state placeholder when no rows are returned", async () => {
    await renderConnected([]);
    expect(await screen.findByText("No settlement history")).toBeTruthy();
  });

  it("shows an alert and retries a rejected fetch", async () => {
    mockUseWallet.mockReturnValue({
      state: "connected",
      address: connectedAddress,
    });
    mockListSettlements
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(response([makeSettlement(1)]));
    render(<SettlementsPage />);

    expect(await screen.findByRole("alert")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(mockListSettlements).toHaveBeenCalledTimes(2));
    expect(await screen.findByTestId("settlement-row")).toBeTruthy();
  });

  it("shows stale-data notice only when lag exceeds 30 seconds", async () => {
    const { unmount } = await renderConnected([makeSettlement(1)], 45);
    expect(
      await screen.findByText(
        "Data may be up to 45 seconds behind the network."
      )
    ).toBeTruthy();

    mockListSettlements.mockResolvedValue(response([makeSettlement(2)], 10));
    unmount();
    render(<SettlementsPage />);
    await screen.findByTestId("settlement-row");
    expect(
      screen.queryByText(/Data may be up to .* seconds behind the network/)
    ).toBeNull();
  });

  it("prompts for a wallet and does not fetch without a connected address", () => {
    mockUseWallet.mockReturnValue({ state: "disconnected" });
    render(<SettlementsPage />);

    expect(
      screen.getByText("Connect your wallet to see your settlements.")
    ).toBeTruthy();
    expect(mockListSettlements).not.toHaveBeenCalled();
  });
});
