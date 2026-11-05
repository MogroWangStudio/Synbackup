// 跨组件共用的小型展示工具（不依赖组件树，纯函数）。
import type { DiffStatus } from "./types";

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
