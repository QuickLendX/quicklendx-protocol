import { Router } from "express";
import * as invoiceController from "../../controllers/v1/invoices";
import * as disputeController from "../../controllers/v1/disputes";
import {
  createQueryValidationMiddleware,
  createParamsValidationMiddleware,
} from "../../middleware/validation";
import {
  getInvoicesQuerySchema,
  invoiceIdParamSchema,
  invoiceIdParamForDisputesSchema,
} from "../../validators/invoices";
import { optionalApiKeyAuth } from "../../middleware/api-key-auth";

const router = Router();

router.get("/", createQueryValidationMiddleware(getInvoicesQuerySchema), optionalApiKeyAuth, invoiceController.getInvoices);
router.get("/:id", createParamsValidationMiddleware(invoiceIdParamSchema), optionalApiKeyAuth, invoiceController.getInvoiceById);
router.get("/:id/disputes", createParamsValidationMiddleware(invoiceIdParamForDisputesSchema), disputeController.getDisputes);

router.post("/", invoiceController.createInvoice);

export default router;
