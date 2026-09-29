import { StatusResponse } from "../types/status";

export class StatusService {
  private static instance: StatusService;
  private isMaintenanceMode: boolean = false;
  private lastIndexedLedger: number = 100000; // Mock starting point

  private constructor() {}

  public static getInstance(): StatusService {
    if (!StatusService.instance) {
      StatusService.instance = new StatusService();
    }
    return StatusService.instance;
  }

  public setMaintenanceMode(enabled: boolean): void {
    this.isMaintenanceMode = enabled;
  }

  public isMaintenanceEnabled(): boolean {
    return this.isMaintenanceMode;
  }

  public async getStatus(): Promise<StatusResponse> {
    const currentLedger = await this.getCurrentBlockchainLedger();
    const lag = currentLedger - this.lastIndexedLedger;
    
    // Logic for degraded mode: if lag > 10 ledgers (approx 50s-1m)
    const isDegraded = lag > 10;
    
    let overallStatus: "operational" | "degraded" | "maintenance" = "operational";
    if (this.isMaintenanceMode) {
      overallStatus = "maintenance";
    } else if (isDegraded) {
      overallStatus = "degraded";
    }

    return {
      status: overallStatus,
      maintenance: this.isMaintenanceMode,
      degraded: isDegraded,
      index_lag: lag,
      last_ledger: this.lastIndexedLedger,
      timestamp: new Date().toISOString(),
      version: process.env.npm_package_version || "1.0.0",
    };
  }

  private mockCurrentLedger: number | null = null;

  public setMockCurrentLedger(ledger: number | null): void {
    this.mockCurrentLedger = ledger;
  }

  private async getCurrentBlockchainLedger(): Promise<number> {
    if (this.mockCurrentLedger !== null) {
      return this.mockCurrentLedger;
    }
    // In a real implementation, this would call a Soroban/Stellar RPC
    // For now, we simulate a slowly advancing ledger
    const now = Date.now();
    const mockCurrent = 100000 + Math.floor((now % 3600000) / 5000); // Advances every 5s
    return mockCurrent;
  }

  // Helper to simulate indexing
  public updateLastIndexedLedger(ledger: number): void {
    this.lastIndexedLedger = ledger;
  }

  public getLastIndexedLedger(): number {
    return this.lastIndexedLedger;
  }

  public async getCurrentLedger(): Promise<number> {
    return this.getCurrentBlockchainLedger();
  }
}

export const statusService = StatusService.getInstance();
