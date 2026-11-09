// 总览页统计项的注册表：页面与"编辑显示项"弹窗共用同一份定义，
// 新增一项只需在这里补一条（键 → 文案 key），两边自动跟上。

/** 规范顺序：对话框按此排列；存储里也只保留这个顺序的子集 */
export const OVERVIEW_STAT_KEYS = ["pairs", "devices", "pending", "memory"] as const;

export type OverviewStatKey = (typeof OVERVIEW_STAT_KEYS)[number];

/** 每项标题的文案 key（overview.stat_* 已存在，沿用） */
export const OVERVIEW_STAT_LABEL: Record<OverviewStatKey, string> = {
  pairs: "overview.stat_pairs",
  devices: "overview.stat_devices",
  pending: "overview.stat_pending",
  memory: "overview.stat_memory",
};

/** 可设置刷新频率的项：其余两项由后端事件驱动，轮询没有意义 */
export const OVERVIEW_POLLED: readonly OverviewStatKey[] = ["devices", "memory"];

/** 刷新频率选项（毫秒；0 = 关闭轮询） */
export const OVERVIEW_INTERVALS = [0, 5000, 15000, 30000, 60000] as const;

/** 轮询下限：低于 3 秒的间隔只会白耗 CPU */
export const MIN_POLL_MS = 3000;
