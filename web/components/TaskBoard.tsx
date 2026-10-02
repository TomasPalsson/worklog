"use client";

import {
  commentOnTicket,
  draftTicketUpdate,
  loadTransitions,
  transitionTicket,
} from "@/app/actions-hub";
import type { TaskRow } from "@/lib/types";
import { TaskCard, type TaskActions } from "./TaskCard";

const realActions: TaskActions = { loadTransitions, transitionTicket, commentOnTicket, draftTicketUpdate };

function Section({ title, tasks, actions }: { title: string; tasks: TaskRow[]; actions: TaskActions }) {
  if (tasks.length === 0) return null;
  return (
    <section className="reg-section">
      <h2>{title}</h2>
      <ul className="task-list">
        {tasks.map((t) => (
          <TaskCard key={t.key} task={t} actions={actions} />
        ))}
      </ul>
    </section>
  );
}

export function TaskBoard({ tasks, actions = realActions }: { tasks: TaskRow[]; actions?: TaskActions }) {
  if (tasks.length === 0) return <p className="reg-lede">No tickets yet.</p>;
  return (
    <>
      <Section title="Assigned" tasks={tasks.filter((t) => t.assigned)} actions={actions} />
      <Section title="Worked this week" tasks={tasks.filter((t) => !t.assigned)} actions={actions} />
    </>
  );
}
