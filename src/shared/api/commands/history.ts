import { invoke } from "../client";
import type { Config, HistoryDay, HistoryPage } from "../types";

export const historyCommands = {
  retryCallback: (jobId: string) => invoke<void>("retry_callback", { jobId }),
  cancelOwnJob: (jobId: string) => invoke<void>("cancel_own_job", { jobId }),
  setHistoryRetention: (days: number) => invoke<Config>("set_history_retention", { days }),
  /** Returns the plain token — the only time it is available. */
  historyDays: () => invoke<HistoryDay[]>("history_days"),
  historyPage: (date: string, page: number, perPage: number, failedOnly: boolean) =>
    invoke<HistoryPage>("history_page", { date, page, perPage, failedOnly }),
  searchHistory: (query: string, page: number, perPage: number) =>
    invoke<HistoryPage>("search_history", { query, page, perPage }),
};
