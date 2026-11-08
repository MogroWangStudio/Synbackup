// 跨组件共用的小型展示工具（不依赖组件树，纯函数）。
import type { DiffStatus } from "./types";

/** 标题栏真实高度（tokens.css 的 --titlebar-h；标题栏不在 zoom 容器内，不随界面缩放）。
    Teleport 到 body 的弹层坐标系 = 真实视口 / zoom，边界换算时用它做分子 */
export function titlebarHeight(): number {
  const v = parseFloat(
    getComputedStyle(document.documentElement).getPropertyValue("--titlebar-h"),
  );
  return Number.isFinite(v) ? v : 40;
}

/** 差异状态 → chip 配色类（差异处理对话框与历史记录共用同一套口径） */
export function statusChipClass(status: DiffStatus): string {
  switch (status) {
    case "both_changed":
    case "type_conflict":
      return "chip-red";
    case "changed_local":
    case "changed_remote":
      return "chip-orange";
    case "new_local":
    case "new_remote":
      return "chip-green";
    default:
      return "chip-gray";
  }
}
