import { notFound } from "next/navigation";
import NewInvoiceForm from "./_components/NewInvoiceForm";

export default function NewInvoicePage() {
  if (process.env.NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE !== "true") {
    notFound();
  }
  return <NewInvoiceForm />;
}
