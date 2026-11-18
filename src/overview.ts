// 总览页统计项的注册表：页面与"编辑显示项"弹窗共用同一份定义，
// 新增一项只需在这里补一条（键 → 文案 key），两边自动跟上。

import type { AppSettings } from "./types";

/** 规范顺序：对话框与卡片都按此排列；存储里也只保留这个顺序的子集 */
export const OVERVIEW_STAT_KEYS = [
  "pairs",
  "devices",
  "pending",
  "size",
  "cpu",
  "gpu",
  "memory_backend",
  "memory_frontend",
] as const;

export type OverviewStatKey = (typeof OVERVIEW_STAT_KEYS)[number];

/** 每项标题的文案 key（overview.stat_* 已存在，沿用） */
export const OVERVIEW_STAT_LABEL: Record<OverviewStatKey, string> = {
  pairs: "overview.stat_pairs",
  devices: "overview.stat_devices",
  pending: "overview.stat_pending",
  size: "overview.stat_size",
  cpu: "overview.stat_cpu",
  gpu: "overview.stat_gpu",
  memory_backend: "overview.stat_memory_backend",
  memory_frontend: "overview.stat_memory_frontend",
};

/** 需要刷新间隔的项：其余项由后端事件驱动，轮询没有意义 */
export const OVERVIEW_POLLED_KEYS = [
  "devices",
  "memory_backend",
  "memory_frontend",
  "cpu",
  "gpu",
  "size",
] as const;

export type OverviewPolledKey = (typeof OVERVIEW_POLLED_KEYS)[number];

export function isPolled(key: OverviewStatKey): key is OverviewPolledKey {
  return (OVERVIEW_POLLED_KEYS as readonly string[]).includes(key);
}

/** 每个可轮询项把间隔存在哪个设置字段里。
    前端 / 后端内存共用同一个频率：一次取数就拿到两项，不必开两套节奏 */
export const OVERVIEW_INTERVAL_FIELD = {
  devices: "overviewDevicesMs",
  memory_backend: "overviewMemoryMs",
  memory_frontend: "overviewMemoryMs",
  cpu: "overviewCpuMs",
  gpu: "overviewGpuMs",
  size: "overviewSizeMs",
} as const satisfies Record<OverviewPolledKey, keyof AppSettings>;

/** "即时更新"档对应的毫秒数（每 0.1 秒取一次） */
export const LIVE_MS = 100;

/** 实时指标档位：即时（0.1 秒）/ 1 秒 / 3 秒起步的老档位都在一列，
    取值节奏交给用户，界面只在后台静默轮询 */
const METRIC_INTERVALS = [0, LIVE_MS, 1000, 3000, 5000, 10000, 15000, 30000, 60000] as const;

/** 设备状态每次取数都要枚举一遍挂载点（系统调用），不给 0.1 秒档 */
const DEVICE_INTERVALS = [0, 1000, 3000, 5000, 10000, 15000, 30000, 60000] as const;

/** GPU 占用取的是"距上次采样这段时间里的平均值"，采得太密只会把噪声
    放大成跳动的数字；1 秒起步也能与任务管理器的刷新节奏对上 */
const GPU_INTERVALS = [0, 1000, 3000, 5000, 10000, 15000, 30000, 60000] as const;

/** 项目总大小要遍历全部本地目录：只给稀疏档位，分钟级起步 */
const SIZE_INTERVALS = [0, 60_000, 300_000, 900_000] as const;

/** 某项可选的刷新频率（毫秒；0 = 关闭轮询） */
export function overviewIntervals(key: OverviewPolledKey): readonly number[] {
  if (key === "size") return SIZE_INTERVALS;
  if (key === "gpu") return GPU_INTERVALS;
  return key === "devices" ? DEVICE_INTERVALS : METRIC_INTERVALS;
}

/** 已保存的间隔折算到档位表里：取不小于原值的最小档（越界就取最大档）。
    旧配置里的 GPU 0.1 秒档不再提供，于是落到 1 秒——不会因为换算
    变成 0 而把这一项悄悄关掉 */
export function normalizeInterval(key: OverviewPolledKey, ms: number): number {
  const list = overviewIntervals(key);
  if (list.includes(ms)) return ms;
  const above = list.find((v) => v > ms);
  return above ?? list[list.length - 1];
}

/** 轮询下限：低于此值即视为关闭（0 档）；具体档位由 overviewIntervals 给出，
    GPU 这类要走系统计数器的项不提供 0.1 秒档 */
export const MIN_POLL_MS = LIVE_MS;
