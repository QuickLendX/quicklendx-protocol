import { Request } from "express";

export interface UserContext {
  userId: string;
}

export type RequestWithUser = Request & {
  user?: UserContext;
};
