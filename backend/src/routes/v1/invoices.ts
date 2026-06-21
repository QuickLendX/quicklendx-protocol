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

const router = Router();

router.get("/", createQueryValidationMiddleware(getInvoicesQuerySchema), invoiceController.getInvoices);
router.get("/:id", createParamsValidationMiddleware(invoiceIdParamSchema), invoiceController.getInvoiceById);
router.get("/:id/disputes", createParamsValidationMiddleware(invoiceIdParamForDisputesSchema), disputeController.getDisputes);

export default router;
