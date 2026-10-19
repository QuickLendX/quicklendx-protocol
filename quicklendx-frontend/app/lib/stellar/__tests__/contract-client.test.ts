import { rpc, xdr, TransactionBuilder, Networks } from "@stellar/stellar-sdk";
import { invokeContract } from "../contract-client";
import { ContractError } from "../contract-errors";

describe("invokeContract Unit Tests with fake rpc.Server", () => {
  const source = "GCXZ7YOZVQ43YJY4PRJBHV7465X6VGD47DQOVNN6NUIVHIHMNBNI4J5L";
  const contractId = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";

  it("simulation error text containing Error(Contract, #1600) throws ContractError { code: 1600 } and does not call signTransaction", async () => {
    const fakeServer = {
      getAccount: jest.fn().mockResolvedValue({ sequence: "100" }),
      simulateTransaction: jest.fn().mockResolvedValue({
        error: "Host invocation failed: Error(Contract, #1600)",
      }),
      prepareTransaction: jest.fn(),
      sendTransaction: jest.fn(),
      getTransaction: jest.fn(),
    } as unknown as rpc.Server;

    const signTransaction = jest.fn();

    await expect(
      invokeContract<string>({
        method: "store_invoice",
        args: [],
        source,
        contractId,
        signTransaction,
        rpc: fakeServer,
      })
    ).rejects.toMatchObject({
      name: "ContractError",
      code: 1600,
      message: "Your business must complete KYC before creating invoices.",
    });

    // Does not call signTransaction
    expect(signTransaction).not.toHaveBeenCalled();
  });

  it("diagnostic event error containing code 1601 throws ContractError { code: 1601 }", async () => {
    // Diagnostic event encoded with code 1601
    const diagEvent = new xdr.DiagnosticEvent({
      inSuccessfulContractCall: false,
      event: new xdr.ContractEvent({
        ext: new (xdr.ExtensionPoint as any)(0),
        contractId: null,
        type: xdr.ContractEventType.system(),
        body: new (xdr.ContractEventBody as any)(
          0,
          new xdr.ContractEventV0({
            topics: [xdr.ScVal.scvSymbol("error")],
            data: xdr.ScVal.scvError(xdr.ScError.sceContract(1601)),
          })
        ),
      }),
    });

    const fakeServer = {
      getAccount: jest.fn().mockResolvedValue({ sequence: "100" }),
      simulateTransaction: jest.fn().mockResolvedValue({
        diagnosticEventsXdr: [diagEvent.toXDR("base64")],
      }),
      prepareTransaction: jest.fn(),
      sendTransaction: jest.fn(),
      getTransaction: jest.fn(),
    } as unknown as rpc.Server;

    const signTransaction = jest.fn();

    await expect(
      invokeContract<string>({
        method: "store_invoice",
        args: [],
        source,
        contractId,
        signTransaction,
        rpc: fakeServer,
      })
    ).rejects.toMatchObject({
      name: "ContractError",
      code: 1601,
      message: "Your KYC is still under review.",
    });

    expect(signTransaction).not.toHaveBeenCalled();
  });

  it("success polls getTransaction until SUCCESS and returns the decoded BytesN<32> as hex", async () => {
    const expectedHash =
      "a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890";
    const invoiceIdBytes = Buffer.from(
      "deadbeefcafebabe0123456789abcdef0123456789abcdef0123456789abcdef",
      "hex"
    );

    // Mock prepared transaction
    const fakePreparedTx = {
      toXDR: () => "fakePreparedXdr",
    };

    let pollCount = 0;

    const fakeServer = {
      getAccount: jest.fn().mockResolvedValue({ sequence: "100" }),
      simulateTransaction: jest.fn().mockResolvedValue({
        // success simulation
        minResourceFee: "100",
      }),
      prepareTransaction: jest.fn().mockResolvedValue(fakePreparedTx),
      sendTransaction: jest.fn().mockResolvedValue({
        status: "PENDING",
        hash: expectedHash,
      }),
      getTransaction: jest.fn().mockImplementation(async () => {
        pollCount++;
        if (pollCount === 1) {
          return { status: rpc.Api.GetTransactionStatus.NOT_FOUND };
        }
        return {
          status: rpc.Api.GetTransactionStatus.SUCCESS,
          returnValue: xdr.ScVal.scvBytes(invoiceIdBytes),
        };
      }),
    } as unknown as rpc.Server;

    // Mock TransactionBuilder.fromXDR
    const origFromXDR = TransactionBuilder.fromXDR;
    TransactionBuilder.fromXDR = jest.fn().mockReturnValue({} as any);

    const signTransaction = jest.fn().mockResolvedValue("signedXdrString");

    try {
      const response = await invokeContract<string>({
        method: "store_invoice",
        args: [],
        source,
        contractId,
        signTransaction,
        rpc: fakeServer,
      });

      expect(response.txHash).toBe(expectedHash);
      expect(response.result).toBe(
        "deadbeefcafebabe0123456789abcdef0123456789abcdef0123456789abcdef"
      );
      expect(pollCount).toBe(2);
      expect(signTransaction).toHaveBeenCalledWith("fakePreparedXdr");
    } finally {
      TransactionBuilder.fromXDR = origFromXDR;
    }
  });

  it("with fake timers, NOT_FOUND for 60 s gives a timeout error", async () => {
    jest.useFakeTimers();

    const fakePreparedTx = {
      toXDR: () => "fakePreparedXdr",
    };

    const fakeServer = {
      getAccount: jest.fn().mockResolvedValue({ sequence: "100" }),
      simulateTransaction: jest
        .fn()
        .mockResolvedValue({ minResourceFee: "100" }),
      prepareTransaction: jest.fn().mockResolvedValue(fakePreparedTx),
      sendTransaction: jest.fn().mockResolvedValue({
        status: "PENDING",
        hash: "timeoutTxHash",
      }),
      getTransaction: jest.fn().mockResolvedValue({
        status: rpc.Api.GetTransactionStatus.NOT_FOUND,
      }),
    } as unknown as rpc.Server;

    const origFromXDR = TransactionBuilder.fromXDR;
    TransactionBuilder.fromXDR = jest.fn().mockReturnValue({} as any);

    const signTransaction = jest.fn().mockResolvedValue("signedXdrString");

    try {
      const promise = invokeContract<string>({
        method: "store_invoice",
        args: [],
        source,
        contractId,
        signTransaction,
        rpc: fakeServer,
        timeoutMs: 60_000,
      });

      const assertion = expect(promise).rejects.toMatchObject({
        name: "ContractError",
        message: "Transaction polling timed out.",
      });

      // Run timers until timeout (61 seconds)
      for (let i = 0; i < 62; i++) {
        await jest.advanceTimersByTimeAsync(1000);
      }

      await assertion;
    } finally {
      TransactionBuilder.fromXDR = origFromXDR;
      jest.useRealTimers();
    }
  });
});
