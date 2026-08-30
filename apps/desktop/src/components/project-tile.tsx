import type { ComponentPropsWithoutRef } from 'react'
import { SmoothCorners } from '@lisse/react'
import type { SmoothCornerOptions } from '@lisse/react'
import { projectInitials, projectTint } from '@/lib/project-identity'
import type { Project } from '@shared/contract'

/**
 * The 32px squares on the project rail, drawn with continuous corners rather
 * than the circular arc `rounded-md` gives — the same curve macOS uses for the
 * app icons most of these tiles end up showing.
 *
 * `radius` tracks `--radius-md` (0.8 x the 0.5rem `--radius`), so the tiles keep
 * the corner size the rest of the chrome uses and only the curve changes. Lisse
 * takes a number, so the token cannot be handed over as a CSS variable.
 */
const TILE_CORNERS: SmoothCornerOptions = { radius: 6.4, smoothing: 0.6 }

/*
 * `autoEffects` defaults to on, which lifts an element's border and box-shadow
 * into an SVG overlay and claims `position` on the parent to hang it from.
 * These tiles have neither, so it stays off: each one gets a `clip-path` and
 * nothing else. That also keeps every SVG `<filter>` out of the tree, which is
 * what puts the WebKit drop-shadow rasterization bug out of reach — worth
 * having when two of Tauri's three webviews are WebKit.
 */

/** Artwork that fills the tile, clipped to the tile's own shape. */
export function TileImage(props: ComponentPropsWithoutRef<'img'>) {
  return (
    <SmoothCorners as="img" corners={TILE_CORNERS} autoEffects={false} {...props} />
  )
}

/** A tinted tile standing in for a project whose artwork has not resolved. */
export function TileSurface(props: ComponentPropsWithoutRef<'span'>) {
  return (
    <SmoothCorners as="span" corners={TILE_CORNERS} autoEffects={false} {...props} />
  )
}

/**
 * The rail's fallback tile. Sizing and type scale come from `className` so the
 * same component serves the 32px rail and any smaller row.
 */
export function TileInitials({
  project,
  className,
}: {
  project: Project
  className?: string
}) {
  return (
    <TileSurface
      // The name is already on the row or the tooltip beside it, so the
      // initials are decoration rather than a second copy for a screen reader.
      aria-hidden
      className={`grid select-none place-items-center font-semibold leading-none ${projectTint(project.path)} ${className ?? ''}`}
    >
      {projectInitials(project.name)}
    </TileSurface>
  )
}
