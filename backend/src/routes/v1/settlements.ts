import { Router } from "express";
import * as settlementsController from "../../controllers/v1/settlements";
import {
  createQueryValidationMiddleware,
  createParamsValidationMiddleware,
} from "../../middleware/validation";
import {
  getSettlementsQuerySchema,
  settlementIdParamSchema,
} from "../../validators/settlements";

const router = Router();

router.get("/", createQueryValidationMiddleware(getSettlementsQuerySchema), settlementsController.getSettlements);
router.get("/:id", createParamsValidationMiddleware(settlementIdParamSchema), settlementsController.getSettlementById);

export default router;
