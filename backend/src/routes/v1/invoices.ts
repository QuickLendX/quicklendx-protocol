import { Router } from "express";
import * as invoiceController from "../../controllers/v1/invoices";
import * as disputeController from "../../controllers/v1/disputes";

const router = Router();

router.get("/", invoiceController.getInvoices);
router.get("/:id", invoiceController.getInvoiceById);
router.get("/:id/disputes", disputeController.getDisputes);

export default router;
