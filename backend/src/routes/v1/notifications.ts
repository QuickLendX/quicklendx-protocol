import { Router, Request, Response } from 'express';
import { notificationService } from '../../services/notificationService';
import { UserNotificationPreferences, NotificationType } from '../../types/contract';
import { z } from 'zod';

const router = Router();

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
router.get('/preferences/:userId', async (req: Request, res: Response) => {
  try {
    const userId = req.params.userId as string;
    const preferences = await notificationService.getUserPreferencesPublic(userId);

    if (!preferences) {
      return res.status(404).json({ error: 'User preferences not found' });
    }

    res.json(preferences);
  } catch (error) {
    console.error('Error fetching preferences:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Update user notification preferences
router.put('/preferences/:userId', async (req: Request, res: Response) => {
  try {
    const userId = req.params.userId as string;
    const updates = updatePreferencesSchema.parse(req.body);

    await notificationService.updateUserPreferences(userId, updates as any);

    res.json({ success: true });
  } catch (error) {
    if (error instanceof z.ZodError) {
      return res.status(400).json({ error: 'Invalid request data', details: error.issues });
    }

    console.error('Error updating preferences:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

// Unsubscribe from all emails (for email links)
router.post('/unsubscribe/:userId', async (req: Request, res: Response) => {
  try {
    const userId = req.params.userId as string;

    await notificationService.updateUserPreferences(userId, {
      email_enabled: false,
    } as any);

    res.json({ success: true, message: 'Successfully unsubscribed from email notifications' });
  } catch (error) {
    console.error('Error unsubscribing:', error);
    res.status(500).json({ error: 'Internal server error' });
  }
});

export default router;