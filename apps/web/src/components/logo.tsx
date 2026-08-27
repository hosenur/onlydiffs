import Image from "next/image"
import { twMerge } from "tailwind-merge"

/**
 * The onlydiffs mark — byte-identical to the icon the desktop build ships, so
 * the site and the app show the same thing. Shared by the header, the footer,
 * and the mobile nav's watermark.
 *
 * A raster illustration rather than an inline SVG, so it goes through
 * next/image for sized variants instead of riding in every page bundle.
 *
 * `sizes` defaults to the small chrome uses; without it next/image assumes the
 * mark is full-bleed and hands a 20px slot a 640w file. The watermark renders
 * an order of magnitude larger and passes its own.
 *
 * The OG route cannot use this — Satori resolves images itself and will not
 * fetch a relative `/_next/image` URL — so it inlines the same file instead.
 */
export const Logo = ({ className, sizes = "32px" }: { className?: string; sizes?: string }) => {
  return (
    <Image
      src="/logo.png"
      alt=""
      aria-hidden
      width={512}
      height={512}
      sizes={sizes}
      className={twMerge("shrink-0", className)}
    />
  )
}
