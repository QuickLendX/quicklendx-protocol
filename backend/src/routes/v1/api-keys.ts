import { Router } from 'express';
import {
  createApiKey,
  listApiKeys,
  getApiKey,
  rotateApiKey,
  revokeApiKey,
  getKeyAuditLogs,
  getScopes,
} from '../../controllers/v1/api-keys';
import { apiKeyAuthMiddleware, requireScopes } from '../../middleware/api-key-auth';

const router = Router();

// Public endpoint to get available scopes (no auth required for discovery)
router.get('/scopes', getScopes);

// All other endpoints require authentication and admin:keys scope
router.use(apiKeyAuthMiddleware);
router.use(requireScopes(['admin:keys']));

// API key management endpoints
router.post('/', createApiKey);
router.get('/', listApiKeys);
router.get('/:id', getApiKey);
router.post('/:id/rotate', rotateApiKey);
router.post('/:id/revoke', revokeApiKey);
router.get('/:id/audit-logs', getKeyAuditLogs);

export default router;
