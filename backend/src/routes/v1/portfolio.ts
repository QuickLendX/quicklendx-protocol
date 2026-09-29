import { Router } from "express";
import * as portfolioController from "../../controllers/v1/portfolio";

const router = Router();

router.get("/", portfolioController.getPortfolio);

export default router;
