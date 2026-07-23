import { Alert } from "../../types/reconciliation";

export interface AlertTransport {
  send(alert: Alert): Promise<void>;
}
