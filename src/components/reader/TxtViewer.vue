<script setup lang="ts">
defineProps<{ text: string }>();
</script>

<template>
  <pre class="text-viewer">{{ text }}</pre>
</template>
