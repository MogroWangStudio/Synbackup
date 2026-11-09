// 总览页统计项的注册表：页面与"编辑显示项"弹窗共用同一份定义，
// 新增一项只需在这里补一条（键 → 文案 key），两边自动跟上。

import type { AppSettings } from "./types";

/** 规范顺序：对话框与卡片都按此排列；存储里也只保留这个顺序的子集 */
export const OVERVIEW_STAT_KEYS = ["pairs", "devices", "pending", "size", "cpu", "gpu", "memory"] as const;

export type OverviewStatKey = (typeof OVERVIEW_STAT_KEYS)[number];

/** 每项标题的文案 key（overview.stat_* 已存在，沿用） */
export const OVERVIEW_STAT_LABEL: Record<OverviewStatKey, string> = {
  pairs: "overview.stat_pairs",
  devices: "overview.stat_devices",
  pending: "overview.stat_pending",
  size: "overview.stat_size",
  cpu: "overview.stat_cpu",
  gpu: "overview.stat_gpu",
  memory: "overview.stat_memory",
};

/** 需要刷新间隔的项：其余项由后端事件驱动，轮询没有意义 */
export const OVERVIEW_POLLED_KEYS = ["devices", "memory", "cpu", "gpu", "size"] as const;

export type OverviewPolledKey = (typeof OVERVIEW_POLLED_KEYS)[number];

export function isPolled(key: OverviewStatKey): key is OverviewPolledKey {
  return (OVERVIEW_POLLED_KEYS as readonly string[]).includes(key);
}

/** 每个可轮询项把间隔存在哪个设置字段里 */
export const OVERVIEW_INTERVAL_FIELD = {
  devices: "overviewDevicesMs",
  memory: "overviewMemoryMs",
  cpu: "overviewCpuMs",
  gpu: "overviewGpuMs",
  size: "overviewSizeMs",
} as const satisfies Record<OverviewPolledKey, keyof AppSettings>;

/** 实时指标的刷新频率：5 秒起，够读又不空转 */
const METRIC_INTERVALS = [0, 5000, 10000, 15000, 30000, 60000] as const;

/** 项目总大小要遍历全部本地目录：只给稀疏档位，分钟级起步 */
const SIZE_INTERVALS = [0, 60_000, 300_000, 900_000] as const;

/** 某项可选的刷新频率（毫秒；0 = 关闭轮询） */
export function overviewIntervals(key: OverviewPolledKey): readonly number[] {
  return key === "size" ? SIZE_INTERVALS : METRIC_INTERVALS;
}

/** 轮询下限：低于 3 秒的间隔只会白耗 CPU */
export const MIN_POLL_MS = 3000;
