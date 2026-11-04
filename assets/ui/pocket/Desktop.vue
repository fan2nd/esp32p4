<script setup lang="ts">
import { View, Text } from "@pocketjs/framework/vue-vapor/components";
import { items } from "./Desktop";
</script>
<template>
  <View class="w-[360px] h-[640px] bg-[#e8f7ff]">
    <View v-for="item in items" :key="item.id" class="absolute items-center justify-center" :style="{ translateX: item.x, translateY: item.y, width: item.width, height: item.height, bgColor: item.background, borderColor: item.border, borderWidth: item.stroke, radius: item.radius }">
      <Text class="text-[#182d44] text-base">{{ item.label }}</Text>
    </View>
  </View>
</template>
