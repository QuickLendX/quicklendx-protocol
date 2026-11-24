import { InvoiceDetailView } from "../../_components/InvoiceDetailView";

export default function InvoiceDetailPage(props: {
  params: Promise<{ id: string }>;
  searchParams?: Promise<{ [key: string]: string | string[] | undefined }>;
}) {
  // The place-bid flow is off unless NEXT_PUBLIC_ENABLE_BID_FLOW=true. It is
  // opt-in because POST /bids authenticates with an API key and an HMAC request
  // signature that a plain browser session cannot produce; see the env docs.
  const bidFlowEnabled = process.env.NEXT_PUBLIC_ENABLE_BID_FLOW === "true";
  return <InvoiceDetailView bidFlowEnabled={bidFlowEnabled} {...props} />;
}
