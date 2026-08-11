// 窗口状态持久化：main 保存尺寸+位置，pet 保存位置。零依赖，localStorage。
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalSize, PhysicalPosition } from '@tauri-apps/api/window'
import { onMounted, onUnmounted } from 'vue'

export interface MainWindowState {
  width: number
  height: number
  x: number
  y: number
}

export interface PetWindowState {
  x: number
  y: number
}

export const MAIN_WINDOW_KEY = 'lingchat_window_main'
export const PET_WINDOW_KEY = 'lingchat_window_pet'

/** 读取 localStorage JSON 值（容错解析失败） */
export function readLocalStorage<T = unknown>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key)
    return raw ? (JSON.parse(raw) as T) : null
  } catch {
    return null
  }
}

function writeLocalStorage(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // 忽略（隐私模式等场景下写入失败不影响功能）
  }
}

/**
 * 坐标合理性校验：多显示器/分辨率变化后，上次保存的坐标可能落到已移除的
 * 显示器上。这里只做粗校验（有限值 + 不至于离谱越界），不精确判断显示器，
 * 避免引入平台差异。
 */
function isPositionUsable(x: number, y: number): boolean {
  return (
    Number.isFinite(x) &&
    Number.isFinite(y) &&
    x >= -2000 &&
    y >= -2000 &&
    x <= 10000 &&
    y <= 10000
  )
}

/**
 * 主窗口状态持久化（仅在 main 窗口调用）。
 *
 * 挂载时恢复上次的尺寸/位置（若在合理范围内）；resize/move 节流保存。
 */
export function useMainWindowState() {
  const cleanups: (() => void)[] = []
  let saveTimer: ReturnType<typeof setTimeout> | null = null

  onMounted(async () => {
    const win = getCurrentWindow()

    // 恢复上次尺寸与位置
    const saved = readLocalStorage<MainWindowState>(MAIN_WINDOW_KEY)
    if (saved && typeof saved.width === 'number' && typeof saved.height === 'number') {
      try {
        await win.setSize(new LogicalSize(saved.width, saved.height))
      } catch (e) {
        console.warn('[WindowState] 恢复主窗口尺寸失败:', e)
      }
      if (isPositionUsable(saved.x, saved.y)) {
        try {
          await win.setPosition(new PhysicalPosition(saved.x, saved.y))
        } catch (e) {
          console.warn('[WindowState] 恢复主窗口位置失败:', e)
        }
      }
    }

    // 统一取窗口最新尺寸/位置（物理像素），节流落盘
    const save = async () => {
      try {
        const size = await win.innerSize()
        const pos = await win.outerPosition()
        writeLocalStorage(MAIN_WINDOW_KEY, {
          width: size.width,
          height: size.height,
          x: pos.x,
          y: pos.y,
        })
      } catch (e) {
        console.warn('[WindowState] 保存主窗口状态失败:', e)
      }
    }
    const debounced = () => {
      if (saveTimer) clearTimeout(saveTimer)
      saveTimer = setTimeout(save, 500)
    }

    const unResized = await win.onResized(debounced)
    const unMoved = await win.onMoved(debounced)
    cleanups.push(unResized, unMoved)
  })

  onUnmounted(() => {
    if (saveTimer) clearTimeout(saveTimer)
    cleanups.forEach((fn) => fn())
  })
}

/**
 * 桌宠窗口位置持久化（仅在 pet 窗口调用）。
 *
 * 创建位置由 Rust `enter_pet` 按传入 position 直接建窗（免闪烁），这里只负责
 * 拖动后实时保存，供下次进入时恢复。
 */
export function usePetWindowState() {
  const cleanups: (() => void)[] = []

  onMounted(async () => {
    const win = getCurrentWindow()
    const save = async () => {
      try {
        const pos = await win.outerPosition()
        writeLocalStorage(PET_WINDOW_KEY, { x: pos.x, y: pos.y })
      } catch (e) {
        console.warn('[WindowState] 保存桌宠位置失败:', e)
      }
    }
    const unMoved = await win.onMoved(save)
    cleanups.push(unMoved)
  })

  onUnmounted(() => {
    cleanups.forEach((fn) => fn())
  })
}
