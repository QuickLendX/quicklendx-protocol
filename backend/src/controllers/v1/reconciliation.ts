import { Request, Response, NextFunction } from "express";
import { ReconciliationWorker } from "../../services/reconciliationWorker";

export const getDriftReports = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const reports = ReconciliationWorker.getAllReports();
    res.json(reports);
  } catch (error) {
    next(error);
  }
};

export const runReconciliation = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const report = await ReconciliationWorker.runReconciliation();
    res.json(report);
  } catch (error) {
    next(error);
  }
};

export const triggerBackfill = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const latestReport = ReconciliationWorker.getLatestReport();
    if (!latestReport) {
      return res.status(400).json({ error: "No drift report available. Run reconciliation first." });
    }
    const result = await ReconciliationWorker.triggerBoundedBackfill(latestReport);
    res.json(result);
  } catch (error) {
    next(error);
  }
};
