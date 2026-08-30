<template>
  <router-view />
  <!-- 将光标特效 teleport 到 body，避免 #app 上的整体缩放（transform: scale）导致坐标偏移 -->
  <Teleport to="body">
    <CursorEffects />
  </Teleport>

  <!-- 全局通知组件（直接从 uiStore 读取状态） -->
  <!-- 与桌宠专用通知组件区分开 -->
  <!-- 弹窗类组件仅主窗口挂载：日志等独立窗口复用 App.vue，不重复弹出 -->
  <Notification v-if="isMainWindow && route.path !== '/pet'" />
  <AchievementToast v-if="isMainWindow" />
  <AdventureUnlockNotify v-if="isMainWindow" />
  <AppDialog v-if="isMainWindow" />
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

import { getAutostartStatus } from './api/services/config'
import { useGameStore } from './stores/modules/game'
import { eventQueue } from './core/events/event-queue'
import CursorEffects from './components/effects/CursorEffects.vue'
import Notification from './components/ui/Notification.vue'
import AchievementToast from './components/ui/AchievementToast.vue'
import AdventureUnlockNotify from './components/ui/AdventureUnlockNotify.vue'
import AppDialog from './components/ui/AppDialog.vue'
import { initUIStore, useUIStore } from './stores/modules/ui/ui'
import { i18n } from './locales'
import { useSettingsStore } from './stores/modules/settings'
import { useLlmProvidersStore } from './stores/modules/llm-providers'
import { useAchievementStore } from './stores/modules/ui/achievement'
import { useDialogStore } from './stores/modules/ui/dialog'
import { useSedentaryReminder } from './composables/useSedentaryReminder'
import { useUpdater } from './composables/useUpdater'
import { useCanDeliver } from './composables/useCanDeliver'
import { useZoom } from './composables/useZoom'
import { useAsrInput } from './composables/useAsrInput'
import { listSystemFonts, getImportedFonts, registerAllImportedFonts } from './api/services/font'

// ─── 激活主动对话投放条件上报（仅在此处挂载一次） ────────────
useCanDeliver()

// 激活 Ctrl+滚轮 UI 全局缩放
useZoom()

// ─── 久坐提醒 ────────────────────────────────────────────────
useSedentaryReminder()

// ─── 全局字体 ────────────────────────────────────────────────
// 把设置中的自定义字体名同步到 <html> 的 --font-app；
// 为空时 base.css 中的回退栈 --font-sans 生效。初始菜单 / 加载页因自带
// 显式 font-family 不会继承此变量，自动保持原有字体。
const uiStore = useUIStore()
const settingsStore = useSettingsStore()

// 等待「入场问候」处理完成（后端在问候生成/跳过时发 entry:greeting-done），用于桌宠 loading。
function waitEntryGreetingDone() {
  return new Promise<void>((resolve) => {
    let settled = false
    let unlisten: (() => void) | null = null
    const finish = () => {
      if (settled) return
      settled = true
      unlisten?.()
      resolve()
    }
    listen('entry:greeting-done', finish).then((u) => {
      if (settled) u()
      else unlisten = u
    })
    setTimeout(finish, 15000)
  })
}

function applyFont(font?: string) {
  // 留空 → 软件默认（base.css 的 --font-sans 原版字体栈）
  document.documentElement.style.setProperty('--font-app', font ? `'${font}'` : '')
}
watch(() => settingsStore.text.fontFamily, applyFont, { immediate: true })

// 提前预取系统字体列表：在应用初始化时即调用一次 Rust 枚举并入内存缓存，
// 避免打开设置页时才触发 IPC 造成可感知的卡顿。注：忽略结果即可，
// SettingsText 进入时直接命中 font.ts 的缓存。
void listSystemFonts()

// 启动时加载导入字体并注册 @font-face 规则，确保用户之前导入的字
// 体在 settings store 恢复字体选择前已可用。
void getImportedFonts().then((fonts) => {
  registerAllImportedFonts(fonts)
})

// ─── 键盘处理 ────────────────────────────────────────────────

const route = useRoute()
const router = useRouter()
const gameStore = useGameStore()

// 仅主窗口挂载全局弹窗（通知/成就/对话确认），日志窗口等复用 App.vue 的窗口不弹
const isMainWindow = getCurrentWindow().label === 'main'

// ASR 全局初始化（仅主窗口一次）：auto_listen 能量监测门控 + 事件监听。
// useAsrInput 状态是模块级单例，GameDialog / ChatInput（桌宠）的 mic 按钮
// 与这里共享同一会话。
if (isMainWindow) {
  useAsrInput()
}

const handleKeyDown = async (event: KeyboardEvent) => {
  if (event.key === 'F11') {
    event.preventDefault()

    // Pet 路由时不允许全屏
    if (route.path === '/pet') {
      return
    }

    try {
      const appWindow = getCurrentWindow()
      const isFullscreen = await appWindow.isFullscreen()
      await appWindow.setFullscreen(!isFullscreen)
    } catch (e) {
      console.error('全屏切换失败:', e)
    }
  }
}

// ─── 关闭确认 ────────────────────────────────────────────────

const dialogStore = useDialogStore()
let saveCompleted = false
let userConfirmedExit = false
let unlistenCloseReady: (() => void) | null = null
let unlistenCloseRequested: (() => void) | null = null

// 处理退出：两个条件都满足时调用 Rust exit_app
function tryExit() {
  if (saveCompleted && userConfirmedExit) {
    invoke('exit_app')
  }
}

onMounted(async () => {
  // 初始化 UI Store（加载角色 tips）
  initUIStore()
  const llmStore = useLlmProvidersStore()

  // ─── 开机自启动 · 启动即桌宠 ─────────────────────────────
  // 只在「系统开机自启」触发（带 --autostart 参数）且开启 boot_as_pet 时才进入桌宠；
  // 手动双击 exe 不带 --autostart，一律走主菜单。
  if (isMainWindow) {
    try {
      const auto = await getAutostartStatus()
      // 全局：无论开机自启 / 手动启动、主菜单 / 桌宠，开启后都在对话场景默认开启自动播放
      if (auto.auto_play) uiStore.autoMode = true
      // 是否以桌宠进入：开机自启且开启桌宠，或（手动启动且开启“以桌宠模式启动”）
      const wantPet =
        (auto.launched_by_autostart && auto.boot_as_pet) ||
        (!auto.launched_by_autostart && auto.startup_pet_mode)
      if (wantPet) {
        const petRoleId = Number(auto.pet_role_id) || 0
        // 进入桌宠前标记「准备中」：在前端 LLM、TTS 服务与入场问候就绪前禁止对话
        uiStore.petReady = false
        uiStore.petBooting = true
        try {
          // 开机自启动直接进入桌宠：先把主窗口切到「置顶桌宠」状态，
          // 避免 loading 圆盘被其他窗口覆盖；随后 PetMode onMounted 会按配置 scale 再调一次。
          await invoke('set_pet_mode', {
            enable: true,
            scale: settingsStore.pet.scale || 1.0,
          }).catch((err) => {
            console.warn('[Autostart] 提前进入桌宠置顶状态失败（非致命）:', err)
          })
          // 1) 只加载默认角色（不触发问候，避免语音生成时 API 未就绪）
          await gameStore.bootAsPet(petRoleId > 0 ? petRoleId : undefined)
          const roleId =
            gameStore.mainRoleId > 0 ? gameStore.mainRoleId : petRoleId > 0 ? petRoleId : null
          if (auto.auto_play) uiStore.autoMode = true
          router.push('/pet')

          // 2) 先让前端 LLM 与外部 TTS 服务就绪（拉起 bat + 探测 + 刷新 TTS）
          //    仅在开启「自动开启 API 服务」时拉起；内置/未配置/未开启时立即就绪
          const ttsReady = auto.auto_start_tts
            ? invoke('autostart_boot_apply', { roleId }).catch(() => {})
            : Promise.resolve()
          await Promise.all([llmStore.load().catch(() => {}), ttsReady])

          // 3) API 就绪、TTS 刷新完成后，再触发「入场问候」，保证语音合成时服务已可用
          if (auto.startup_greeting) {
            const wait = waitEntryGreetingDone()
            invoke('notify_player_entry').catch((err) =>
              console.warn('[Entry] 问候触发失败（非致命）:', err),
            )
            await wait
          }

          // 4) 保证 loading 至少展示一段时间（给刷新/落盘留缓冲）
          await new Promise((resolve) => setTimeout(resolve, 2000))
        } catch (e) {
          console.error('[Autostart] 启动即桌宠初始化失败:', e)
        } finally {
          // 5) loading 结束、角色显示后再恢复事件队列消费，让问候/对话按顺序处理与播放
          eventQueue.resume()
          uiStore.petReady = true
          uiStore.petBooting = false
        }
      } else if (auto.auto_start_tts) {
        // 正常启动（非桌宠）：也按全局开关自动拉起/刷新外部 TTS API 服务
        invoke('autostart_boot_apply', { roleId: null }).catch((e) =>
          console.warn('[Autostart] 正常启动拉起 TTS 失败（非致命）:', e),
        )
      }
    } catch (e) {
      console.error('[Autostart] 启动即桌宠初始化失败:', e)
    }
  }

  // 启动时自动弹出独立日志窗口（仅主窗口触发，开关在日志页设置）
  if (
    getCurrentWindow().label === 'main' &&
    localStorage.getItem('lingchat_log_window_auto_open') === '1'
  ) {
    invoke('open_log_window').catch((e) => console.error('自动打开日志窗口失败:', e))
  }

  // 预加载 LLM 提供商配置，避免主界面因 store 未加载而误判未选择模型
  llmStore.load().catch((e) => console.error('加载 LLM 提供商失败:', e))

  // 供成就系统控制台测试用，在 window 对象中注册一些方法
  const achievementStore = useAchievementStore()
  ;(window as any).requestAchievementUnlock = (data: any) =>
    achievementStore.notifyBackendUnlock(data)
  ;(window as any).showAchievement = (data: any) => achievementStore.addAchievement(data)
  // 成就系统启动WebSocket监听
  achievementStore.listenForUnlocks()

  // 注册 F11 全屏快捷键
  window.addEventListener('keydown', handleKeyDown)

  // ─── 关闭确认逻辑 ──────────────────────────────────────────

  // 1. 监听 Rust 存档完成事件
  unlistenCloseReady = await listen('app:close-ready', () => {
    saveCompleted = true
    tryExit()
  })

  // 2. 拦截窗口关闭请求（仅主窗口需要确认，其他窗口正常关闭）
  unlistenCloseRequested = await getCurrentWindow().onCloseRequested(
    async (event: { preventDefault: () => void }) => {
      if (getCurrentWindow().label !== 'main') return

      event.preventDefault()

      // 重置状态
      saveCompleted = false
      userConfirmedExit = false

      if (route.path === '/chat') {
        const confirmed = await dialogStore.confirm(
          i18n.global.t('common.exitMessage'),
          i18n.global.t('common.exitTitle'),
        )
        if (!confirmed) return // 用户取消，窗口保持打开
      }

      userConfirmedExit = true
      tryExit()
    },
  )
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleKeyDown)
  if (unlistenCloseReady) unlistenCloseReady()
  if (unlistenCloseRequested) unlistenCloseRequested()
})
</script>

<style>
:root {
  /*全局变量*/
  --accent-color: #79d9ff;
  --menu-max-width: 1100px;
  --menu-max-width-half: 550px;
  /* 一个生动的天蓝色，可以根据你的品牌调整 */
}

/* 全局样式和字体 */
body,
html {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: transparent;
}

#app {
  width: 100vw;
  height: 100vh;
}
</style>
