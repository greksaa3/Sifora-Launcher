<script setup lang="ts">
/**
 * The layered backdrop.
 *
 * Four absolutely-positioned layers, each driven purely by CSS custom
 * properties that the theme writes:
 *
 *   base    — the gradient the theme asked for
 *   aurora  — slow drifting colour blobs (skipped when motion is reduced)
 *   pattern — the repeating tile (grid, dots, stars, ...)
 *   image   — the user's own picture, if one is selected
 *
 * A grain/vignette pass is composited on top. Everything here is deliberately
 * free of `filter: blur()` and `mix-blend-mode`: those force a full-screen
 * re-render on every animated frame, which measured at roughly 2.5 CPU cores
 * of idle load before they were removed.
 */
import { computed } from 'vue'

import { toAssetUrl } from '@/lib/ipc'
import ArtworkLayer from '@/components/ArtworkLayer.vue'
import type { ThemeSpec } from '@/types'

const props = defineProps<{
  theme: ThemeSpec | null
  /** Absolute path of the imported background image, if any. */
  imagePath?: string | null
  /** `on` or `off`; `off` freezes every ambient animation. */
  motion?: 'on' | 'off'
}>()

const imageUrl = computed(() => {
  const path = props.imagePath
  if (!path) return null
  try {
    return toAssetUrl(path)
  } catch {
    return null
  }
})

/** A blur filter over a full-screen layer is costly, so it is opt-in. */
const imageBlur = computed(() => {
  const blur = props.theme?.tokens.background.image_blur ?? 0
  return blur > 0.5 ? `blur(${blur}px)` : undefined
})

const kind = computed(() => props.theme?.tokens.background.kind ?? 'aurora')
const auroraStrength = computed(() => props.theme?.tokens.decor.aurora ?? 0)
const grain = computed(() => props.theme?.tokens.decor.grain ?? 0)
const vignette = computed(() => props.theme?.tokens.decor.vignette ?? 0)
const scanlines = computed(() => props.theme?.tokens.decor.scanlines ?? false)

const artwork = computed(() => props.theme?.tokens.background.artwork ?? 'none')
const artworkStrength = computed(() => props.theme?.tokens.background.artwork_strength ?? 0)
</script>

<template>
  <div class="sf-backdrop" aria-hidden="true">
    <div class="sf-backdrop__layer sf-backdrop__base" :data-kind="kind" />
    <ArtworkLayer :artwork="artwork" :strength="artworkStrength" />
    <div
      class="sf-backdrop__layer sf-backdrop__aurora"
      v-show="auroraStrength > 0.01"
      :style="{ opacity: auroraStrength }"
      :data-motion="motion"
    />
    <div class="sf-backdrop__layer sf-backdrop__pattern" />
    <div
      v-if="imageUrl"
      class="sf-backdrop__layer sf-backdrop__image"
      :style="{ backgroundImage: `url(&quot;${imageUrl}&quot;)`, filter: imageBlur }"
    />
    <div class="sf-backdrop__layer sf-backdrop__grain" :style="{ opacity: grain }" />
    <div
      class="sf-backdrop__layer sf-backdrop__scanlines"
      :style="{ opacity: scanlines ? 0.6 : 0 }"
    />
    <div class="sf-backdrop__layer sf-backdrop__vignette" :style="{ opacity: vignette }" />
  </div>
</template>

<style scoped>
.sf-backdrop {
  position: fixed;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
  z-index: 0;
  background: var(--sf-bg-1);
}

.sf-backdrop__layer {
  position: absolute;
  inset: -6%;
  will-change: transform;
}

/* --------------------------------------------------------------- base */

.sf-backdrop__base {
  background:
    radial-gradient(52% 44% at 16% 10%, var(--sf-bg-2), transparent 68%),
    radial-gradient(46% 40% at 86% 20%, var(--sf-bg-3), transparent 70%),
    radial-gradient(64% 52% at 56% 102%, var(--sf-bg-4), transparent 74%),
    radial-gradient(38% 34% at 6% 94%, var(--sf-bg-5), transparent 72%),
    linear-gradient(var(--sf-bg-angle), var(--sf-bg-1), var(--sf-bg-5));
}

.sf-backdrop__base[data-kind='solid'] {
  background: linear-gradient(var(--sf-bg-angle), var(--sf-bg-1), var(--sf-bg-1));
}

.sf-backdrop__base[data-kind='linear'],
.sf-backdrop__base[data-kind='image'] {
  background: linear-gradient(
    var(--sf-bg-angle),
    var(--sf-bg-1) 0%,
    var(--sf-bg-2) 24%,
    var(--sf-bg-3) 46%,
    var(--sf-bg-4) 70%,
    var(--sf-bg-5) 100%
  );
}

.sf-backdrop__base[data-kind='radial'] {
  background:
    radial-gradient(120% 90% at 50% 8%, var(--sf-bg-2), var(--sf-bg-3) 52%, var(--sf-bg-4)),
    linear-gradient(var(--sf-bg-angle), var(--sf-bg-1), var(--sf-bg-4));
}

.sf-backdrop__base[data-kind='conic'] {
  background: conic-gradient(
    from var(--sf-bg-angle) at 50% 42%,
    var(--sf-bg-1),
    var(--sf-bg-2),
    var(--sf-bg-3),
    var(--sf-bg-4),
    var(--sf-bg-1)
  );
}

/* ------------------------------------------------------------- aurora */

/*
 * This layer used to carry `filter: blur(30px)` together with
 * `mix-blend-mode: screen` and an infinite full-screen animation. That
 * combination cost about 2.5 CPU cores on a static screen: a blurred layer
 * cannot be cached while it transforms, the blend forces the whole backdrop
 * beneath it to be re-composited, and every `backdrop-filter` panel on top
 * then has to re-sample that changing surface each frame.
 *
 * None of that is needed for the look. The blobs are radial gradients, so they
 * are already soft; mixing the stops towards white reproduces what the
 * `screen` blend used to contribute; and a transform-only animation on a
 * promoted layer is close to free.
 */
.sf-backdrop__aurora {
  background:
    radial-gradient(
      42% 34% at 24% 32%,
      color-mix(in srgb, var(--sf-bg-2) 45%, white),
      transparent 72%
    ),
    radial-gradient(
      36% 30% at 76% 62%,
      color-mix(in srgb, var(--sf-bg-3) 45%, white),
      transparent 74%
    ),
    radial-gradient(
      44% 36% at 58% 18%,
      color-mix(in srgb, var(--sf-bg-4) 40%, white),
      transparent 70%
    );
  opacity: 0;
  transform: translateZ(0);
}

.sf-backdrop__aurora[data-motion='on'] {
  opacity: inherit;
  animation: sf-drift 38s var(--sf-ease) infinite alternate;
  /* Promote so the animation only recomposites a cached texture. */
  will-change: transform;
}

.sf-backdrop[data-motion='on'] .sf-backdrop__aurora {
  animation-duration: calc(var(--sf-dur-slow) * 34);
}

/* Idle/performance rules live in `styles/main.css` — they have to reach every
   component, which scoped styles cannot do. */

@keyframes sf-drift {
  0% {
    transform: translate3d(-3%, -2%, 0) scale(1);
  }
  50% {
    transform: translate3d(4%, 3%, 0) scale(1.08);
  }
  100% {
    transform: translate3d(-2%, 4%, 0) scale(1.02);
  }
}

/* ------------------------------------------------------------ pattern */

.sf-backdrop__pattern {
  background-image: var(--sf-pattern);
  background-size: calc(48px * var(--sf-pattern-scale));
  opacity: 0.5;
  background-repeat: repeat;
}

/* -------------------------------------------------------------- image */

.sf-backdrop__image {
  background-repeat: no-repeat;
  background-position: center;
  background-size: var(--sf-img-fit);
  opacity: var(--sf-img-opacity);
}

/* --------------------------------------------------- grain / vignette */

.sf-backdrop__grain {
  background-image: var(--sf-pattern);
  background-size: 160px 160px;
  opacity: 0.04;
}

.sf-backdrop__scanlines {
  background: var(--sf-scanlines);
  background-size: 100% 3px;
}

.sf-backdrop__vignette {
  background: radial-gradient(
    120% 100% at 50% 45%,
    transparent 38%,
    rgb(0 0 0 / 0.35) 78%,
    rgb(0 0 0 / 0.65) 100%
  );
}

@media (prefers-reduced-motion: reduce) {
  .sf-backdrop__aurora {
    animation: none;
  }
}
</style>