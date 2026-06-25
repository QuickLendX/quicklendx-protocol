import { Router } from "express";
import * as invoiceController from "../../controllers/v1/invoices";
import * as disputeController from "../../controllers/v1/disputes";
import {
  createQueryValidationMiddleware,
  createParamsValidationMiddleware,
} from "../../middleware/validation";
import {
  getInvoicesQuerySchema,
  invoiceRouteIdParamSchema,
  invoiceRouteIdParamForDisputesSchema,
} from "../../validators/invoices";
import { apiKeyAuthMiddleware } from "../../middleware/api-key-auth";

const router = Router();

router.get("/", apiKeyAuthMiddleware, createQueryValidationMiddleware(getInvoicesQuerySchema), invoiceController.getInvoices);
router.get("/:id", apiKeyAuthMiddleware, createParamsValidationMiddleware(invoiceRouteIdParamSchema), invoiceController.getInvoiceById);
router.get("/:id/disputes", apiKeyAuthMiddleware, createParamsValidationMiddleware(invoiceRouteIdParamForDisputesSchema), disputeController.getDisputes);

router.post("/", apiKeyAuthMiddleware, invoiceController.createInvoice);

export default router;
