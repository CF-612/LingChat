<template>
  <!-- 背景分类管理（子文件夹 = 子分类）：选项卡 + 新建 + 删除 -->
  <div class="mb-4 flex flex-wrap items-center gap-2">
    <button
      class="rounded-full border px-3 py-1 text-xs font-semibold transition-all"
      :class="
        modelValue === ALL_CATEGORY
          ? 'bg-brand/80 border-brand text-white'
          : 'border-white/20 bg-white/10 text-white/70 hover:bg-white/20'
      "
      @click="$emit('update:modelValue', ALL_CATEGORY)"
    >
      {{ $t("settings.background.scene.categoryAll") }}
    </button>
    <button
      v-for="cat in categories"
      :key="cat"
      class="rounded-full border px-3 py-1 text-xs font-semibold transition-all"
      :class="
        modelValue === cat
          ? 'bg-brand/80 border-brand text-white'
          : 'border-white/20 bg-white/10 text-white/70 hover:bg-white/20'
      "
      @click="$emit('update:modelValue', cat)"
    >
      {{ cat }}
    </button>

    <!-- 新建分类 -->
    <div class="flex items-center gap-1">
      <input
        :value="name"
        :placeholder="$t('settings.background.scene.categoryNamePlaceholder')"
        class="w-28 rounded-lg border border-white/15 bg-black/30 px-2 py-1 text-xs text-white focus:border-indigo-400 focus:outline-none"
        @input="$emit('update:name', ($event.target as HTMLInputElement).value)"
        @keyup.enter="$emit('create')"
      />
      <button
        class="rounded-full border border-indigo-400 bg-indigo-500/80 px-2.5 py-1 text-xs font-semibold text-white hover:bg-indigo-500"
        @click="$emit('create')"
      >
        {{ $t("settings.background.scene.categoryAdd") }}
      </button>
    </div>

    <!-- 删除当前选中的分类（非"全部"时显示） -->
    <button
      v-if="modelValue !== ALL_CATEGORY && !readOnly"
      class="rounded-full border border-red-400/40 bg-red-500/20 px-2.5 py-1 text-xs font-semibold text-red-300 hover:bg-red-500/30"
      @click="$emit('delete')"
    >
      {{ $t("settings.background.scene.categoryDelete") }}
    </button>
  </div>
</template>

<script setup lang="ts">
import { ALL_CATEGORY } from "@/composables/settings/useBackgroundLibrary";

defineProps<{
  /** 当前选中的分类 */
  modelValue: string;
  /** 可选的分类列表（不含虚拟分类） */
  categories: string[];
  /** 「新建分类」输入框的内容 */
  name: string;
  /** 虚拟分类（插件）下不允许增删 */
  readOnly: boolean;
}>();

defineEmits<{
  (e: "update:modelValue", value: string): void;
  (e: "update:name", value: string): void;
  (e: "create"): void;
  (e: "delete"): void;
}>();
</script>
