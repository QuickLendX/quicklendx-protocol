import { Router } from "express";
import * as settlementsController from "../../controllers/v1/settlements";

const router = Router();

router.get("/", settlementsController.getSettlements);
router.get("/:id", settlementsController.getSettlementById);

export default router;
