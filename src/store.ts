import { reactive } from "vue";
import { api } from "./api";
import type {
  AppSettings,
  DiffItem,
  HistoryEntry,
  PendingReview,
  SyncPair,
  VolumeInfo,
} from "./types";

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  text: string;
}

export interface DiffDialogState {
  pairId: string;
  pairName: string;
  deviceLabel: string;
  scannedAt: number;
  items: DiffItem[];
  source: "pending" | "manual";
}

export type ViewKey = "overview" | "pairs" | "devices" | "history" | "settings";

export const store = reactive({
  booted: false,
  view: "overview" as ViewKey,
  settings: {
    language: "zh-CN",
    launchAtLogin: false,
    autoScanOnInsert: true,
    autoSyncNoConflict: false,
    notifyOnInsert: true,
    pollIntervalSecs: 3,
  } as AppSettings,
  pairs: [] as SyncPair[],
  volumes: [] as VolumeInfo[],
  pending: [] as PendingReview[],
  history: [] as HistoryEntry[],
  appInfo: null as null | { dataDir: string; version: string },
  toasts: [] as Toast[],
  diffDialog: null as DiffDialogState | null,
  editor: null as null | { pair: SyncPair | null },
  syncingPairIds: [] as string[],
});

let toastSeq = 1;

export function toast(kind: Toast["kind"], text: string) {
  const id = toastSeq++;
  store.toasts.push({ id, kind, text });
  window.setTimeout(() => dismissToast(id), 4500);
}

export function dismissToast(id: number) {
  const i = store.toasts.findIndex((t) => t.id === id);
  if (i >= 0) store.toasts.splice(i, 1);
}

export async function refreshAll() {
  const [cfg, volumes, pending, history, appInfo] = await Promise.all([
    api.getConfig(),
    api.listVolumes(),
    api.getPendingReviews(),
    api.getHistory(200),
    api.getAppInfo(),
  ]);
  store.settings = cfg.settings;
  store.pairs = cfg.pairs;
  store.volumes = volumes;
  store.pending = pending;
  store.history = history;
  store.appInfo = appInfo;
  store.booted = true;
}

export async function refreshConfig() {
  const cfg = await api.getConfig();
  store.settings = cfg.settings;
  store.pairs = cfg.pairs;
}

export async function refreshVolumes() {
  store.volumes = await api.listVolumes();
}

export async function refreshPending() {
  store.pending = await api.getPendingReviews();
}

export async function refreshHistory() {
  store.history = await api.getHistory(200);
}
