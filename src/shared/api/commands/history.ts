import { invoke } from "../client";
import type { Config, HistoryDay, HistoryPage } from "../types";

export const historyCommands = {
  setHistoryRetention: (days: number) => invoke<Config>("set_history_retention", { days }),
  /** Returns the plain token — the only time it is available. */
  historyDays: () => invoke<HistoryDay[]>("history_days"),
  historyPage: (date: string, page: number, perPage: number, failedOnly: boolean) =>
    invoke<HistoryPage>("history_page", { date, page, perPage, failedOnly }),
};
