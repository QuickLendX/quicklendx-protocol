import { settlementsToCsv } from "./csv";
import type { Settlement } from "./data";

const baseRow: Settlement = {
  id: "settlement-1",
  invoice_id: '=HYPERLINK("http://x")',
  amount: "9007199254740993",
  payer: "GABC",
  recipient: "GXYZ",
  timestamp: 1767225600,
  status: "Paid",
};

describe("settlementsToCsv", () => {
  it("emits the exact header and neutralizes spreadsheet formulas", () => {
    const csv = settlementsToCsv([baseRow]);

    expect(csv.split("\r\n")[0]).toBe(
      "id,invoice_id,amount,status,timestamp_iso,payer,recipient"
    );
    expect(csv).toContain(
      `"'=HYPERLINK(""http://x"")",9007199254740993,Paid,2026-01-01T00:00:00.000Z`
    );
  });

  it("quotes values containing commas, quotes, and newlines", () => {
    const csv = settlementsToCsv([
      {
        ...baseRow,
        payer: "GABC, Inc",
        recipient: 'Line "one"\nLine two',
      },
    ]);

    expect(csv).toContain('"GABC, Inc","Line ""one""\nLine two"');
  });

  it.each(["+SUM(A1:A2)", "-1+1", "@SUM(A1)", "\tformula", "\rformula"])(
    "prefixes a dangerous leading character %p",
    (value) => {
      expect(
        settlementsToCsv([{ ...baseRow, invoice_id: value }]).split("\r\n")[1]
      ).toContain(`'${value}`);
    }
  );
});
