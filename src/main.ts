import { createApp } from "vue";
import pinia from "./stores";
import { initializeEventProcessors } from "./core/events";
import {
  initializeTauriEventListeners,
  initializeWindowActivity,
} from "./api/tauri-events";

import App from "./App.vue";
import "./assets/styles/base.css";
import "./assets/styles/variables.css";
import { i18n } from "./locales";

// WebSocket handlers 保留用于未来剧本模式参考
// import "./api/websocket/handlers/script-handler";
// import "./api/websocket/handlers/adventure-handler";

import { getCurrentWindow } from '@tauri-apps/api/window'
import router from "./router";
import { autoConfigureCpuPerformance } from "./api/services/cpu-perf";
import { initAudioOutputManager } from "./utils/audioOutputManager";

// 仅主窗口启动时清除加载过渡标记，避免设置窗口等其他窗口误清除
if (getCurrentWindow().label === 'main') {
  localStorage.removeItem('lingchat_loading_shown')
}

const app = createApp(App);

initializeEventProcessors();
initializeTauriEventListeners();
// 按窗口 label 初始化活动状态（main/pet 活动，log/settings 等不消费 AI 事件）
initializeWindowActivity();

app.use(pinia);
app.use(i18n);
app.use(router);

// 独立日志窗口：通过 index.html?window=log 打开时直接进入日志路由
// 独立桌宠窗口：通过 index.html?window=pet 打开时直接进入桌宠路由
const queryWindow = new URLSearchParams(window.location.search).get('window')
if (queryWindow === 'log') {
  router.replace('/log-window');
} else if (queryWindow === 'pet') {
  router.replace('/pet');
}

app.mount("#app");

// 初始化全局音频输出设备管理器（需 pinia 就绪）
initAudioOutputManager();

// 延迟执行 CPU 画质自适应，确保 pinia store 已就绪
setTimeout(autoConfigureCpuPerformance, 1000);
