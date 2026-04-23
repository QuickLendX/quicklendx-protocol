import { Router } from "express";
import * as invoiceController from "../../controllers/v1/invoices";

const router = Router();

router.get("/", invoiceController.getInvoices);
router.get("/:id", invoiceController.getInvoiceById);

export default router;
