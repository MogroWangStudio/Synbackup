import { createApp } from "vue";
import App from "./App.vue";
import { store } from "./store";
import { applyUiPrefs } from "./theme";
import "./styles/tokens.css";
import "./styles/base.css";

// 平台标识早于首帧写入，标题栏据此决定是否绘制窗口按钮（macOS 用系统红绿灯）。
// 与 tauri.<platform>.conf.json 的窗口装饰配置一一对应。
const ua = navigator.userAgent;
document.documentElement.dataset.os = /Mac OS X|Macintosh/.test(ua)
  ? "mac"
  : /Windows/.test(ua)
    ? "windows"
    : "linux";

// 外观 / 字体 / 缩放同样早于首帧应用。store.settings 的初始值已合并
// localStorage 偏好（同步读），后端配置返回前不会出现浅色 → 深色之类的闪变。
applyUiPrefs(store.settings);

createApp(App).mount("#app");
