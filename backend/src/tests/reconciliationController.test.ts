import request from 'supertest';
import express from 'express';
import { runReconciliation } from '../controllers/v1/reconciliation';
import { ReconciliationWorker } from '../services/reconciliationWorker';

const app = express();
app.use(express.json());
app.post('/reconciliation/run', runReconciliation);

app.use((err: any, req: any, res: any, next: any) => {
  res.status(500).json({ error: { message: err.message, code: 'INTERNAL_ERROR' } });
});

jest.mock('../services/reconciliationWorker', () => ({
  ReconciliationWorker: {
    runReconciliation: jest.fn(),
    isReconciliationRunning: jest.fn(),
  },
}));

describe('reconciliation controller - runReconciliation', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('should run reconciliation successfully', async () => {
    (ReconciliationWorker.isReconciliationRunning as jest.Mock).mockReturnValue(false);
    (ReconciliationWorker.runReconciliation as jest.Mock).mockResolvedValue({
      timestamp: 123456,
      totalRecordsChecked: 10,
      driftCount: 0,
      drifts: []
    });

    const res = await request(app).post('/reconciliation/run');
    expect(res.status).toBe(200);
    expect(res.body.totalRecordsChecked).toBe(10);
  });

  it('should return 409 if reconciliation is already in progress (checked beforehand)', async () => {
    (ReconciliationWorker.isReconciliationRunning as jest.Mock).mockReturnValue(true);

    const res = await request(app).post('/reconciliation/run');
    expect(res.status).toBe(409);
    expect(res.body.error.code).toBe('CONFLICT');
  });

  it('should return 409 if reconciliation is already in progress (thrown by worker)', async () => {
    (ReconciliationWorker.isReconciliationRunning as jest.Mock).mockReturnValue(false);
    (ReconciliationWorker.runReconciliation as jest.Mock).mockRejectedValue(new Error('Reconciliation already in progress'));

    const res = await request(app).post('/reconciliation/run');
    expect(res.status).toBe(409);
    expect(res.body.error.code).toBe('CONFLICT');
  });

  it('should return 502 if report contains an error (downstream failure)', async () => {
    (ReconciliationWorker.isReconciliationRunning as jest.Mock).mockReturnValue(false);
    (ReconciliationWorker.runReconciliation as jest.Mock).mockResolvedValue({
      timestamp: 123456,
      totalRecordsChecked: 0,
      driftCount: 0,
      drifts: [],
      error: 'RPC Connection Failed'
    });

    const res = await request(app).post('/reconciliation/run');
    expect(res.status).toBe(502);
    expect(res.body.error.code).toBe('BAD_GATEWAY');
  });

  it('should return 500 for other worker errors', async () => {
    (ReconciliationWorker.isReconciliationRunning as jest.Mock).mockReturnValue(false);
    (ReconciliationWorker.runReconciliation as jest.Mock).mockRejectedValue(new Error('RPC connection failed'));

    const res = await request(app).post('/reconciliation/run');
    expect(res.status).toBe(500);
    expect(res.body.error.code).toBe('INTERNAL_ERROR');
  });
});
