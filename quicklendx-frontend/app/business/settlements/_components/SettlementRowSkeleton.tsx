export function SettlementRowSkeleton() {
  return (
    <tr
      aria-hidden="true"
      data-testid="settlement-skeleton-row"
      className="settlement-row-skeleton"
    >
      {["Date", "Invoice", "Amount", "Status", "Action"].map((label) => (
        <td key={label} data-label={label}>
          <span className="skeleton-bar" />
        </td>
      ))}
    </tr>
  );
}
