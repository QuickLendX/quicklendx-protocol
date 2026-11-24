import { Router, Request, Response } from 'express';
import { notificationService } from '../../services/notificationService';
import { UserNotificationPreferences, NotificationType } from '../../types/contract';
import { requireUserAuth, getUser } from '../../middleware/userAuth';
import { createParamsValidationMiddleware } from '../../middleware/validation';
import { userIdParamSchema } from '../../validators/shared';
import {
  verifyUnsubscribeToken,
  maskEmail,
} from '../../services/notificationTokens';
import { z } from 'zod';

const router = Router();
const validateUserIdParam = createParamsValidationMiddleware(userIdParamSchema);

/**
 * Seam for resolving principal. Defaults to reading authenticated user from request.
 */
export function resolvePrincipal(req: Request): string | null {
  try {
    return getUser(req);
  } catch {
    return null;
  }
}

// Schema for updating preferences
const updatePreferencesSchema = z.object({
  email_enabled: z.boolean().optional(),
  email_address: z.string().email().optional(),
  notifications: z.object({
    [NotificationType.InvoiceFunded]: z.boolean().optional(),
    [NotificationType.PaymentReceived]: z.boolean().optional(),
    [NotificationType.DisputeOpened]: z.boolean().optional(),
    [NotificationType.DisputeResolved]: z.boolean().optional(),
  }).optional(),
});

// Get user notification preferences
router.get('/preferences/:userId', validateUserIdParam, requireUserAuth, async (req: Request, res: Response) => {
  try {
    const userId = req.params.userId as string;
    const principal = resolvePrincipal(req);

    // Ownership check: must equal authenticated principal
    if (!principal || principal !== userId) {
      return res.status(403).json({
        error: { message: 'Forbidden', code: 'FORBIDDEN' },
      });
    }

    const preferences = await notificationService.getUserPreferencesPublic(userId);
    if (!preferences) {
      return res.status(404).json({
        error: { message: 'User preferences not found', code: 'NOT_FOUND' },
      });
    }

    // Owner gets full email address
    res.json(preferences);
  } catch (error) {
    console.error('Error fetching preferences:', error);
    res.status(500).json({
      error: { message: 'Internal server error', code: 'INTERNAL_ERROR' },
    });
  }
});

// Update user notification preferences
router.put('/preferences/:userId', validateUserIdParam, requireUserAuth, async (req: Request, res: Response) => {
  try {
    const userId = req.params.userId as string;
    const principal = resolvePrincipal(req);

    // Ownership check: must equal authenticated principal
    if (!principal || principal !== userId) {
      return res.status(403).json({
        error: { message: 'Forbidden', code: 'FORBIDDEN' },
      });
    }

    const updates = updatePreferencesSchema.parse(req.body);

    // Changing email_address does NOT take effect immediately: requires verification
    if (updates.email_address !== undefined) {
      notificationService.requestEmailChange(userId, updates.email_address);

      const { email_address, ...otherUpdates } = updates;
      if (Object.keys(otherUpdates).length > 0) {
        notificationService.updateUserPreferences(userId, otherUpdates as any);
      }

      return res.status(202).json({ verification_required: true });
    }

    notificationService.updateUserPreferences(userId, updates as any);
    res.json({ success: true });
  } catch (error) {
    if (error instanceof z.ZodError) {
      return res.status(400).json({
        error: { message: 'Invalid request data', code: 'INVALID_PARAM', details: error.issues },
      });
    }

    console.error('Error updating preferences:', error);
    res.status(500).json({
      error: { message: 'Internal server error', code: 'INTERNAL_ERROR' },
    });
  }
});

// Verify and activate pending email address
const verifyEmailHandler = async (req: Request, res: Response) => {
  try {
    const userId = req.params.userId as string;
    const token = (req.body?.token || req.query?.token) as string;

    if (!token) {
      return res.status(403).json({
        error: { message: 'Invalid or missing verification token', code: 'INVALID_TOKEN' },
      });
    }

    const result = notificationService.verifyAndActivateEmail(userId, token);
    if (!result.success) {
      return res.status(result.status).json({
        error: { message: result.error || 'Verification failed', code: result.error || 'INVALID_TOKEN' },
      });
    }

    res.status(200).json({ success: true, message: 'Email address verified successfully' });
  } catch (error) {
    console.error('Error verifying email:', error);
    res.status(500).json({
      error: { message: 'Internal server error', code: 'INTERNAL_ERROR' },
    });
  }
};

router.post('/preferences/:userId/email/verify', validateUserIdParam, verifyEmailHandler);
router.post('/preferences/:userId/verify', validateUserIdParam, verifyEmailHandler);

// Unsubscribe from all emails (for email links)
router.post('/unsubscribe/:userId', validateUserIdParam, async (req: Request, res: Response) => {
  try {
    const userId = req.params.userId as string;
    const token = (req.query.token || req.body?.token) as string;

    // Unguessable signed HMAC token verification
    if (!token || !verifyUnsubscribeToken(userId, token)) {
      return res.status(403).json({
        error: { message: 'Invalid or missing unsubscribe token', code: 'INVALID_UNSUBSCRIBE_TOKEN' },
      });
    }

    notificationService.updateUserPreferences(userId, {
      email_enabled: false,
    } as any);

    res.json({
      success: true,
      email_enabled: false,
      message: 'Successfully unsubscribed from email notifications',
    });
  } catch (error) {
    console.error('Error unsubscribing:', error);
    res.status(500).json({
      error: { message: 'Internal server error', code: 'INTERNAL_ERROR' },
    });
  }
});

export default router;
