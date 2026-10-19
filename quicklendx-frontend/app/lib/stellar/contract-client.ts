import {
  rpc,
  xdr,
  Contract,
  TransactionBuilder,
  Account,
  scValToNative,
} from "@stellar/stellar-sdk";
import { getNetworkConfig } from "./network";
import { ContractError, getContractErrorMessage } from "./contract-errors";

export interface InvokeContractOptions {
  method: string;
  args: xdr.ScVal[];
  source: string; /* G... address */
  signTransaction: (xdr: string) => Promise<string>;
  contractId?: string;
  rpc?: rpc.Server;
  timeoutMs?: number; /* default 60_000 */
}

export interface InvokeContractResult<T> {
  result: T;
  txHash: string;
}

/**
 * Extracts a contract error code if present in error strings, simulation results, or diagnostic events.
 */
export function extractContractErrorCode(
  errorOrSimulation: unknown
): number | null {
  if (!errorOrSimulation) return null;

  // 1. Check string messages / representations
  const errorStr =
    typeof errorOrSimulation === "string"
      ? errorOrSimulation
      : (errorOrSimulation as any)?.error ||
        (errorOrSimulation as any)?.message ||
        JSON.stringify(errorOrSimulation);

  if (typeof errorStr === "string") {
    // Matches "Error(Contract, #1600)" or "Error(Contract, 1600)" or "code: 1600"
    const match =
      /Error\(Contract,\s*#?(\d+)\)/.exec(errorStr) ||
      /code\s*#?(\d+)/i.exec(errorStr) ||
      /"contractCode":\s*(\d+)/.exec(errorStr);
    if (match) {
      return parseInt(match[1], 10);
    }
  }

  // 2. Check diagnostic events if present on simulation
  // 2. Check diagnostic events if present on simulation
  const diagnosticEventsXdr = (errorOrSimulation as any)?.diagnosticEventsXdr;
  if (Array.isArray(diagnosticEventsXdr)) {
    for (const eventBase64 of diagnosticEventsXdr) {
      try {
        const diagEvent =
          typeof eventBase64 === "string"
            ? xdr.DiagnosticEvent.fromXDR(eventBase64, "base64")
            : (eventBase64 as xdr.DiagnosticEvent);
        const dataVal = (diagEvent as any)
          ?.event?.()
          ?.body?.()
          ?.value?.()
          ?.data?.();
        if (dataVal) {
          const arm =
            typeof dataVal.arm === "function" ? dataVal.arm() : dataVal._arm;
          if (arm === "error" || dataVal._arm === "error") {
            const err =
              typeof dataVal.error === "function"
                ? dataVal.error()
                : dataVal._value;
            if (err?.switch?.()?.name === "sceContract") {
              return err.contractCode?.();
            }
          }
        }
      } catch {
        // Continue checking other events
      }
    }
  }

  // 3. Check resultXdr or error ScVal if present
  const resultErrorXdr =
    (errorOrSimulation as any)?.resultErrorXdr ||
    (errorOrSimulation as any)?.resultXdr;
  if (resultErrorXdr && typeof resultErrorXdr === "string") {
    try {
      const txResult = xdr.TransactionResult.fromXDR(resultErrorXdr, "base64");
      const results = txResult.result?.()?.results?.();
      if (results && results.length > 0) {
        for (const opRes of results) {
          const tr = opRes.tr?.();
          const hostRes = tr?.invokeHostFunctionResult?.();
          if (hostRes) {
            const val = (hostRes as any).value?.();
            if (val && typeof val === "object") {
              const arm = typeof val.arm === "function" ? val.arm() : val._arm;
              if (arm === "error") {
                const scErr =
                  typeof val.error === "function" ? val.error() : val._value;
                if (scErr?.switch?.()?.name === "sceContract") {
                  return scErr.contractCode?.();
                }
              }
            }
          }
        }
      }
    } catch {
      // Ignore parse failure
    }
  }

  return null;
}

/**
 * Invokes a Soroban smart contract method.
 * Handles simulation -> assemble/prepare -> wallet signing -> sendTransaction -> polling getTransaction.
 */
export async function invokeContract<T>(
  opts: InvokeContractOptions
): Promise<InvokeContractResult<T>> {
  const { method, args, source, signTransaction, timeoutMs = 60_000 } = opts;

  const networkConfig = getNetworkConfig();
  const contractId = opts.contractId || networkConfig.contractId;
  const server = opts.rpc || new rpc.Server(networkConfig.rpcUrl);

  // 1. Build initial invocation transaction
  // Fetch account sequence from network if available, otherwise default to sequence 1
  let account: Account;
  try {
    const accResponse: any = await server.getAccount(source);
    const seq =
      typeof accResponse?.sequenceNumber === "function"
        ? accResponse.sequenceNumber()
        : accResponse?.sequence?.toString() || "1";
    account = new Account(source, seq);
  } catch {
    account = new Account(source, "1");
  }

  const contract = new Contract(contractId);
  const operation = contract.call(method, ...args);

  const tx = new TransactionBuilder(account, {
    fee: "100",
    networkPassphrase: networkConfig.passphrase,
  })
    .addOperation(operation)
    .setTimeout(300)
    .build();

  // 2. Simulate transaction
  const simResponse = await server.simulateTransaction(tx);

  // Check for simulation error
  if (rpc.Api.isSimulationError(simResponse)) {
    const code = extractContractErrorCode(simResponse);
    if (code !== null) {
      throw new ContractError(getContractErrorMessage(code), code, simResponse);
    }
    const errorMsg = (simResponse as any).error || "Contract simulation failed";
    throw new ContractError(errorMsg, undefined, simResponse);
  }

  // Double check error code in diagnostic events or simulation results even if not flagged error
  const possibleCode = extractContractErrorCode(simResponse);
  if (possibleCode !== null) {
    throw new ContractError(
      getContractErrorMessage(possibleCode),
      possibleCode,
      simResponse
    );
  }

  // 3. Assemble / Prepare transaction
  const preparedTx = await server.prepareTransaction(tx);

  // 4. Sign transaction via wallet
  let signedXdr: string;
  try {
    signedXdr = await signTransaction(preparedTx.toXDR());
  } catch (err: any) {
    if (err instanceof ContractError) throw err;
    throw new ContractError("Signature request declined.", undefined, err);
  }

  // 5. Send transaction to network
  const signedTx = TransactionBuilder.fromXDR(
    signedXdr,
    networkConfig.passphrase
  );
  const sendResponse = await server.sendTransaction(signedTx as any);

  if (sendResponse.status === "ERROR") {
    const code = extractContractErrorCode(sendResponse);
    if (code !== null) {
      throw new ContractError(
        getContractErrorMessage(code),
        code,
        sendResponse
      );
    }
    throw new ContractError(
      (sendResponse as any).errorResult || "Failed to submit transaction",
      undefined,
      sendResponse
    );
  }

  const txHash = sendResponse.hash;

  // 6. Poll getTransaction until SUCCESS, FAILED, or timeout
  const startTime = Date.now();
  const pollIntervalMs = 1000;

  while (Date.now() - startTime < timeoutMs) {
    const txStatus = await server.getTransaction(txHash);

    if (txStatus.status === rpc.Api.GetTransactionStatus.SUCCESS) {
      // Decode result from returnValue or resultXdr
      let decodedResult: any = undefined;

      if ((txStatus as any).returnValue) {
        decodedResult = scValToNative((txStatus as any).returnValue);
      } else if (txStatus.resultXdr) {
        try {
          const tr = xdr.TransactionResult.fromXDR(
            txStatus.resultXdr.toXDR("base64"),
            "base64"
          );
          const results = tr.result().results();
          if (results.length > 0) {
            const hostRes = results[0].tr().invokeHostFunctionResult();
            const val: any = hostRes.value();
            if (val) {
              if (Buffer.isBuffer(val) || val instanceof Uint8Array) {
                decodedResult = Buffer.from(val).toString("hex");
              } else {
                decodedResult = scValToNative(val as any);
              }
            }
          }
        } catch {
          // If custom parsing fails, fallback
        }
      }

      // Convert Buffer to hex if result is raw bytes (like BytesN<32>)
      if (Buffer.isBuffer(decodedResult)) {
        decodedResult = decodedResult.toString("hex");
      }

      return {
        result: decodedResult as T,
        txHash,
      };
    }

    if (txStatus.status === rpc.Api.GetTransactionStatus.FAILED) {
      const code = extractContractErrorCode(txStatus);
      if (code !== null) {
        throw new ContractError(getContractErrorMessage(code), code, txStatus);
      }
      throw new ContractError(
        "Transaction execution failed",
        undefined,
        txStatus
      );
    }

    // Wait 1 second before next poll
    await new Promise((resolve) => setTimeout(resolve, pollIntervalMs));
  }

  throw new ContractError("Transaction polling timed out.", undefined, {
    txHash,
  });
}
