describe("Stellar link builders", () => {
  const originalEnv = { ...process.env };
  let links = {} as typeof import("../links");
  let explorerBase = "";

  beforeEach(() => {
    process.env.NEXT_PUBLIC_NETWORK = "public";
    delete process.env.NEXT_PUBLIC_RPC_URL;
    delete process.env.NEXT_PUBLIC_HORIZON_URL;
    delete process.env.NEXT_PUBLIC_CONTRACT_ID;
    jest.resetModules();
    links = require("../links");
    explorerBase = require("../network").getNetworkConfig().explorerBase;
  });

  afterAll(() => {
    process.env = { ...originalEnv };
  });

  it("builds an explorer URL for a 64-character transaction hash", () => {
    expect(links.explorerTxUrl("a".repeat(64))).toBe(
      `${explorerBase}/tx/${"a".repeat(64)}`
    );
  });

  it.each(["../../account/GBAD", "abc", "A".repeat(63)])(
    "rejects invalid transaction hashes: %s",
    (hash) => expect(links.explorerTxUrl(hash)).toBeNull()
  );

  it("accepts a correctly shaped Stellar account and rejects a short one", () => {
    const address = "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K";
    expect(links.explorerAccountUrl(address)).toBe(
      `${explorerBase}/account/${address}`
    );
    expect(
      links.explorerAccountUrl(
        "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ"
      )
    ).toBeNull();
  });

  it("validates contract IDs and invoice paths", () => {
    const contractId = `C${"A".repeat(55)}`;
    expect(links.explorerContractUrl(contractId)).toBe(
      `${explorerBase}/contract/${contractId}`
    );
    expect(links.explorerContractUrl("XYZ")).toBeNull();
    expect(links.invoicePath("0xAbC")).toBe("/business/invoices/0xAbC");
    expect(links.invoicePath("0x1/../x")).toBeNull();
    expect(links.invoicePath("INV-8201")).toBeNull();
  });
});
