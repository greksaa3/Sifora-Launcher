<script setup lang="ts">
/**
 * Large hand-built backdrops.
 *
 * A palette change alone leaves a theme recognisably the same product in a
 * different colour. These give each one a silhouette: you know which theme is
 * running before you read a word.
 *
 * Every shape is generated here from a seeded pseudo-random walk, so nothing
 * is downloaded, nothing belongs to anyone, and the artwork is reproducible.
 *
 * They are plain SVG rendered once. Nothing here animates, blurs or blends —
 * the launcher already established that those cost a full CPU core on a
 * machine with no GPU.
 */
import { computed } from 'vue'

import type { Artwork } from '@/types'

const props = defineProps<{
  artwork: Artwork
  strength: number
}>()

/**
 * Deterministic PRNG. A fixed seed means the same theme always draws the same
 * picture; a seed derived from the artwork keeps two themes from sharing one.
 */
function rng(seed: number): () => number {
  let s = seed >>> 0 || 1
  return () => {
    s ^= s << 13
    s ^= s >>> 17
    s ^= s << 5
    return ((s >>> 0) % 100000) / 100000
  }
}

const SEEDS: Record<Exclude<Artwork, 'none'>, number> = {
  sunburst: 0x5f3a,
  halftone: 0x1b7d,
  blocks: 0x2c91,
  circuit: 0x4e2b,
  waves: 0x71aa,
  confetti: 0x33cf,
  bubbles: 0x6d18,
  contours: 0x0a94,
}

/**
 * Colours come from the theme's own custom properties rather than being parsed
 * here. The artwork then re-tints itself with whatever theme is active, and
 * light/dark legibility is handled by opacity alone instead of by the renderer
 * having to know which mode it is in.
 */
const STROKE = 'var(--sf-accent)'
const FILL = 'var(--sf-accent)'

/**
 * A darker and a lighter relative of the accent.
 *
 * Backdrops need internal contrast to read as drawn shapes rather than a
 * single wash, and deriving both from the accent keeps the artwork on-palette
 * whatever theme is active.
 */
const DEEP = 'color-mix(in srgb, var(--sf-accent) 55%, black)'
const BRIGHT = 'color-mix(in srgb, var(--sf-accent) 30%, white)'

const body = computed(() => {
  const art = props.artwork
  if (art === 'none') return ''
  const r = rng(SEEDS[art])
  const W = 1600
  const H = 1000

  switch (art) {
    /* Comic-book rays fanning out from behind the centre of the window. */
    case 'sunburst': {
      const rays: string[] = []
      const cx = W / 2
      const cy = H * 0.44
      const count = 30
      for (let i = 0; i < count; i++) {
        const a0 = (i / count) * Math.PI * 2
        // Uneven wedges read as drawn rather than machine-generated.
        const spread = 0.05 + r() * 0.05
        const len = 1500 + r() * 800
        const x1 = cx + Math.cos(a0 - spread) * len
        const y1 = cy + Math.sin(a0 - spread) * len
        const x2 = cx + Math.cos(a0 + spread) * len
        const y2 = cy + Math.sin(a0 + spread) * len
        const wide = i % 3 === 0
        rays.push(
          `<path d="M${cx.toFixed(1)} ${cy.toFixed(1)}L${x1.toFixed(1)} ${y1.toFixed(1)}L${x2.toFixed(1)} ${y2.toFixed(1)}Z" fill="${wide ? BRIGHT : DEEP}" fill-opacity="${wide ? 0.5 : 0.16}"/>`,
        )
      }
      // A few solid wedges on top, the way a comic panel breaks up its burst.
      for (let i = 0; i < 5; i++) {
        const a0 = r() * Math.PI * 2
        const spread = 0.03 + r() * 0.02
        const len = 1400 + r() * 600
        const x1 = cx + Math.cos(a0 - spread) * len
        const y1 = cy + Math.sin(a0 - spread) * len
        const x2 = cx + Math.cos(a0 + spread) * len
        const y2 = cy + Math.sin(a0 + spread) * len
        rays.push(
          `<path d="M${cx.toFixed(1)} ${cy.toFixed(1)}L${x1.toFixed(1)} ${y1.toFixed(1)}L${x2.toFixed(1)} ${y2.toFixed(1)}Z" fill="${DEEP}" fill-opacity="0.2"/>`,
        )
      }
      return `<g>${rays.join('')}</g>`
    }

    /* Printed-comic dot screen, thinning towards the top. */
    case 'halftone': {
      const dots: string[] = []
      const step = 26
      for (let y = 0; y < H + step; y += step) {
        for (let x = 0; x < W + step; x += step) {
          const depth = (y / H) * 9 + (x / W) * 3
          const radius = Math.max(0.4, depth * 0.62)
          dots.push(
            `<circle cx="${x}" cy="${y}" r="${radius.toFixed(2)}" fill="${FILL}"/>`,
          )
        }
      }
      return `<g opacity="0.5">${dots.join('')}</g>`
    }

    /* Chunky isometric blocks, like stacked cubes seen from an angle. */
    case 'blocks': {
      const blocks: string[] = []
      for (let i = 0; i < 34; i++) {
        const size = 46 + r() * 96
        const x = r() * W
        const y = r() * H
        const skew = size * 0.5
        const top = `${x.toFixed(0)},${y.toFixed(0)} ${(x + skew).toFixed(0)},${(y - skew * 0.5).toFixed(0)} ${(x + size).toFixed(0)},${y.toFixed(0)} ${(x + size - skew).toFixed(0)},${(y + skew * 0.5).toFixed(0)}`
        const left = `${x.toFixed(0)},${y.toFixed(0)} ${(x + size - skew).toFixed(0)},${(y + skew * 0.5).toFixed(0)} ${(x + size - skew).toFixed(0)},${(y + size + skew * 0.5).toFixed(0)} ${x.toFixed(0)},${(y + size).toFixed(0)}`
        const shade = 0.1 + r() * 0.22
        blocks.push(
          `<polygon points="${top}" fill="${FILL}" fill-opacity="${shade.toFixed(2)}"/>` +
            `<polygon points="${left}" fill="${STROKE}" fill-opacity="${(shade * 1.5).toFixed(2)}"/>`,
        )
      }
      return `<g>${blocks.join('')}</g>`
    }

    /* Circuit traces with pads and vias. */
    case 'circuit': {
      const traces: string[] = []
      for (let i = 0; i < 26; i++) {
        let x = Math.round(r() * W)
        let y = Math.round(r() * H)
        const parts: string[] = [`M${x} ${y}`]
        const steps = 3 + Math.floor(r() * 5)
        for (let s = 0; s < steps; s++) {
          const length = 40 + r() * 150
          if (s % 2 === 0) {
            x += r() > 0.5 ? length : -length
          } else {
            y += r() > 0.5 ? length : -length
          }
          parts.push(`L${x.toFixed(0)} ${y.toFixed(0)}`)
        }
        traces.push(
          `<path d="${parts.join('')}" fill="none" stroke="${STROKE}" stroke-width="${(2 + r() * 3).toFixed(1)}" stroke-opacity="0.4" stroke-linecap="round" stroke-linejoin="round"/>`,
        )
        traces.push(`<circle cx="${x.toFixed(0)}" cy="${y.toFixed(0)}" r="${(4 + r() * 4).toFixed(1)}" fill="${FILL}" fill-opacity="0.55"/>`)
      }
      return `<g>${traces.join('')}</g>`
    }

    /* Soft overlapping waves, for anything watery. */
    case 'waves': {
      const waves: string[] = []
      for (let i = 0; i < 9; i++) {
        const base = 130 + i * 108
        const amp = 26 + r() * 44
        let d = `M0 ${base}`
        for (let x = 0; x <= W; x += 80) {
          const y = base + Math.sin((x / 160) * Math.PI + i) * amp
          d += ` L${x} ${y.toFixed(1)}`
        }
        d += ` L${W} ${H} L0 ${H} Z`
        waves.push(`<path d="${d}" fill="${i % 2 ? FILL : STROKE}" fill-opacity="${(0.1 - i * 0.008).toFixed(3)}"/>`)
      }
      return `<g>${waves.join('')}</g>`
    }

    /* Scattered confetti shards for anything celebratory. */
    case 'confetti': {
      const shards: string[] = []
      for (let i = 0; i < 90; i++) {
        const x = r() * W
        const y = r() * H
        const w = 10 + r() * 26
        const h = w * (0.3 + r() * 0.5)
        const angle = r() * 360
        shards.push(
          `<rect x="${x.toFixed(0)}" y="${y.toFixed(0)}" width="${w.toFixed(0)}" height="${h.toFixed(0)}" rx="3" fill="${r() > 0.5 ? FILL : STROKE}" fill-opacity="${(0.18 + r() * 0.3).toFixed(2)}" transform="rotate(${angle.toFixed(0)} ${x.toFixed(0)} ${y.toFixed(0)})"/>`,
        )
      }
      return `<g>${shards.join('')}</g>`
    }

    /* Rising bubbles for anything submerged. */
    case 'bubbles': {
      const bubbles: string[] = []
      for (let i = 0; i < 46; i++) {
        const radius = 8 + r() * 46
        bubbles.push(
          `<circle cx="${(r() * W).toFixed(0)}" cy="${(r() * H).toFixed(0)}" r="${radius.toFixed(1)}" fill="none" stroke="${STROKE}" stroke-width="2.5" stroke-opacity="${(0.16 + r() * 0.3).toFixed(2)}"/>`,
        )
      }
      return `<g>${bubbles.join('')}</g>`
    }

    /* Flowing topographic contour lines. */
    case 'contours': {
      const lines: string[] = []
      for (let i = 0; i < 22; i++) {
        const base = i * 48
        let d = `M0 ${base}`
        for (let x = 0; x <= W; x += 40) {
          const y =
            base +
            Math.sin(x / 260 + i * 0.6) * 42 +
            Math.sin(x / 90 + i) * 14
          d += ` L${x} ${y.toFixed(1)}`
        }
        lines.push(
          `<path d="${d}" fill="none" stroke="${STROKE}" stroke-width="1.8" stroke-opacity="${(0.3 - i * 0.008).toFixed(3)}"/>`,
        )
      }
      return `<g>${lines.join('')}</g>`
    }

    default:
      return ''
  }
})
</script>

<template>
  <svg
    v-if="artwork !== 'none' && body"
    class="sf-artwork"
    viewBox="0 0 1600 1000"
    preserveAspectRatio="xMidYMid slice"
    aria-hidden="true"
    :style="{ opacity: strength }"
  >
    <!-- Generated shapes only; there is no path by which a theme supplies markup. -->
    <g v-html="body" />
  </svg>
</template>

<style scoped>
.sf-artwork {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
  z-index: 1;
}
</style>