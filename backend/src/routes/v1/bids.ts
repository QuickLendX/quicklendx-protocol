import { Router } from "express";
import * as bidController from "../../controllers/v1/bids";

const router = Router();

router.get("/", bidController.getBids);
router.get("/best/:invoiceId", bidController.getBestBid);
router.get("/top/:invoiceId", bidController.getTopBids);

export default router;
