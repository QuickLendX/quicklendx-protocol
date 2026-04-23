import { Router } from "express";
import * as bidController from "../../controllers/v1/bids";

const router = Router();

router.get("/", bidController.getBids);

export default router;
