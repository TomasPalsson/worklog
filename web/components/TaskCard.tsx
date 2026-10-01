"use client";

import type {
  loadTransitions as loadTransitionsAction,
  transitionTicket as transitionTicketAction,
  commentOnTicket as commentOnTicketAction,
  draftTicketUpdate as draftTicketUpdateAction,
} from "@/app/actions-hub";
import type { TaskRow } from "@/lib/types";

export interface TaskActions {
  loadTransitions: typeof loadTransitionsAction;
  transitionTicket: typeof transitionTicketAction;
  commentOnTicket: typeof commentOnTicketAction;
  draftTicketUpdate: typeof draftTicketUpdateAction;
}

export function TaskCard(_props: { task: TaskRow; actions: TaskActions }): never {
  throw new Error("not implemented");
}
