import { InvoiceDetailView } from "../../_components/InvoiceDetailView";

export default function InvoiceDetailPage(props: {
  params: Promise<{ id: string }>;
  searchParams?: Promise<{ [key: string]: string | string[] | undefined }>;
}) {
  return <InvoiceDetailView {...props} />;
}
