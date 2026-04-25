import { Router } from "express";
import * as bidController from "../../controllers/v1/bids";
import { createQueryValidationMiddleware } from "../../middleware/validation";
import { getBidsQuerySchema } from "../../validators/bids";

const router = Router();

router.get("/", createQueryValidationMiddleware(getBidsQuerySchema), bidController.getBids);

export default router;
