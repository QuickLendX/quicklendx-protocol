export const DEFAULT_DECIMALS = 7;

/** "12.5" -> "125000000". Yana dawo da null idan shigarwar ba daidai ba ce. */
export function toBaseUnits(
  input: string,
  decimals = DEFAULT_DECIMALS
): string | null {
  const m = /^(\d+)(?:\.(\d*))?$/.exec(input.trim());
  if (!m) return null;
  const frac = m[2] ?? "";
  if (frac.length > decimals) return null;
  return (m[1] + frac.padEnd(decimals, "0")).replace(/^0+(?=\d)/, "");
}

export function lineTotal(quantity: string, unitPrice: string): string {
  return (BigInt(quantity) * BigInt(unitPrice)).toString();
}
