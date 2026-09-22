import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useUIStore } from "@/stores/modules/ui/ui";
import { useDialogStore } from "@/stores/modules/ui/dialog";
import {
  getBackgroundImages,
  uploadBackgroundImage,
  listBackgroundCategories,
  createBackgroundCategory,
  deleteBackgroundCategory,
} from "@/api/services/background";
import type { BackgroundImageInfo } from "@/types";

/** 「全部」不是磁盘上的分类，表示不按分类过滤 */
export const ALL_CATEGORY = "全部";
/** 插件来源的场景所属的虚拟分类，不对应磁盘子文件夹，不可写 */
export const VIRTUAL_CATEGORY = "插件";
/** 兜底分类：背景不在任何子文件夹中 */
const ROOT_CATEGORY = "根目录";
/** 允许上传的背景图片扩展名 */
export const ALLOWED_BACKGROUND_EXTENSIONS = [
  ".jpg",
  ".jpeg",
  ".png",
  ".webp",
  ".bmp",
  ".svg",
  ".tif",
  ".gif",
];

interface UseBackgroundLibraryOptions {
  /** 背景库发生变化（新建/删除分类、上传）后的回调，用于同步场景列表 */
  onChanged?: () => Promise<void> | void;
}

/**
 * 背景图片库：图片列表、子分类（= 子文件夹）的读写，以及按分类过滤场景所需的映射。
 */
export function useBackgroundLibrary(options: UseBackgroundLibraryOptions = {}) {
  const { t } = useI18n();
  const uiStore = useUIStore();
  const dialogStore = useDialogStore();

  const backgroundList = ref<BackgroundImageInfo[]>([]);
  const backgroundCategories = ref<string[]>([]);
  const currentBackgroundCategory = ref<string>(ALL_CATEGORY);
  /** 「新建分类」输入框的内容 */
  const newCategoryName = ref("");

  const isBackgroundCategoryReadOnly = computed(
    () => currentBackgroundCategory.value === VIRTUAL_CATEGORY,
  );
  const writableBackgroundCategories = computed(() =>
    backgroundCategories.value.filter((category) => category !== VIRTUAL_CATEGORY),
  );

  /**
   * 场景的背景 → 所属分类 映射（url → category），用于按分类过滤场景卡片。
   * 先按完整 url 精确匹配；再按文件名（basename）兜底匹配，兼顾
   * 前后端对路径分隔符/大小写的表示差异，避免子分类标签下场景被误归为“根目录”。
   */
  function categoryOfBackground(url: string): string {
    if (!url) return ROOT_CATEGORY;
    const matched = backgroundList.value.find((b) => b.url === url);
    if (matched?.category) return matched.category;
    const base = (url.split(/[\\/]/).pop() || "").toLowerCase();
    const byName = backgroundList.value.find((b) => {
      const bBase = (b.url || "").split(/[\\/]/).pop() || "";
      return bBase.toLowerCase() === base;
    });
    return byName?.category || ROOT_CATEGORY;
  }

  async function fetchBackgrounds(): Promise<BackgroundImageInfo[]> {
    try {
      const data = await getBackgroundImages();
      return data.map((background: BackgroundImageInfo) => ({
        title: background.title || "Untitled",
        url: background.url || "",
        time: background.time,
        // 保留所属子分类（子文件夹名），否则按分类选项卡过滤场景时会全部落到“根目录”
        category: background.category,
      }));
    } catch (error) {
      console.error("Failed to fetch background list:", error);
      return [];
    }
  }

  async function loadBackgroundCategories(): Promise<void> {
    try {
      const cats = await listBackgroundCategories();
      backgroundCategories.value = cats;
      if (
        currentBackgroundCategory.value !== ALL_CATEGORY &&
        !cats.includes(currentBackgroundCategory.value)
      ) {
        currentBackgroundCategory.value = ALL_CATEGORY;
      }
    } catch (error) {
      console.error("加载背景分类失败", error);
    }
  }

  /** 重新拉取背景图片与分类；本地增删后统一走这里，保证两侧数据一致 */
  async function refreshBackground(): Promise<void> {
    backgroundList.value = await fetchBackgrounds();
    await loadBackgroundCategories();
  }

  async function refreshAndNotifyScenes(): Promise<void> {
    await refreshBackground();
    await options.onChanged?.();
  }

  /** 用输入框中的名字新建分类；成功返回 true（失败时已提示用户） */
  async function createCategory(): Promise<boolean> {
    const name = newCategoryName.value.trim();
    if (!name) {
      await dialogStore.alert(t("settings.background.scene.categoryNameEmpty"));
      return false;
    }
    try {
      await createBackgroundCategory(name);
      newCategoryName.value = "";
      await refreshAndNotifyScenes();
      uiStore.showSuccess({
        title: t("settings.background.scene.categoryCreated"),
        message: t("settings.background.scene.categoryCreatedMsg", { name }),
        duration: 3000,
      });
      return true;
    } catch (error: any) {
      console.error("创建分类失败:", error);
      await dialogStore.alert(t("settings.background.scene.categoryCreateFail"));
      return false;
    }
  }

  /** 删除当前分类：其中的背景移回根目录，场景不会被删除 */
  async function deleteCurrentCategory(): Promise<boolean> {
    const category = currentBackgroundCategory.value;
    if (!category || category === ALL_CATEGORY || category === VIRTUAL_CATEGORY) return false;
    const confirmed = await dialogStore.confirm(
      t("settings.background.scene.categoryDeleteConfirmMove", { name: category }),
    );
    if (!confirmed) return false;
    try {
      const movedCount = await deleteBackgroundCategory(category, "move_to_root");
      currentBackgroundCategory.value = ALL_CATEGORY;
      await refreshAndNotifyScenes();
      uiStore.showSuccess({
        title: t("settings.background.scene.categoryDeleted"),
        message: t("settings.background.scene.categoryDeletedMoved", {
          name: category,
          count: movedCount,
        }),
        duration: 3000,
      });
      return true;
    } catch (error) {
      console.error("删除分类失败:", error);
      await dialogStore.alert(t("settings.background.scene.categoryDeleteFail"));
      return false;
    }
  }

  /**
   * 上传背景图片到当前分类（「全部」表示根目录）。
   * 后端会自动把新背景注册为场景，因此成功后同样需要刷新场景列表。
   */
  async function uploadBackground(file: File): Promise<boolean> {
    if (isBackgroundCategoryReadOnly.value) return false;

    const fileExt = file.name.slice(file.name.lastIndexOf(".")).toLowerCase();
    if (!ALLOWED_BACKGROUND_EXTENSIONS.includes(fileExt)) {
      await dialogStore.alert(
        t("settings.background.upload.invalidFormat", {
          formats: ALLOWED_BACKGROUND_EXTENSIONS.join(", "),
        }),
      );
      return false;
    }

    try {
      const buf = await file.arrayBuffer();
      const category =
        currentBackgroundCategory.value === ALL_CATEGORY
          ? undefined
          : currentBackgroundCategory.value;
      await uploadBackgroundImage(file.name, new Uint8Array(buf), category);
      await refreshAndNotifyScenes();
      return true;
    } catch (error) {
      console.error("上传失败", error);
      await dialogStore.alert(t("settings.background.upload.failed"));
      return false;
    }
  }

  return {
    backgroundList,
    backgroundCategories,
    currentBackgroundCategory,
    newCategoryName,
    isBackgroundCategoryReadOnly,
    writableBackgroundCategories,
    categoryOfBackground,
    refreshBackground,
    createCategory,
    deleteCurrentCategory,
    uploadBackground,
  };
}
