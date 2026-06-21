import { Router } from "express";

const router = Router();

router.post("/callbacks", (req, res) => {
  res.status(202).json({
    accepted: true,
    message: "Webhook callback accepted",
  });
});

router.all("/callbacks", (req, res) => {
  res.status(405).json({
    error: {
      message: "Method not allowed",
      code: "METHOD_NOT_ALLOWED",
    },
  });
});

export default router;
