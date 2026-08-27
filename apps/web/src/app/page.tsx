import type { Metadata } from "next"
import { Geist } from "next/font/google"
import Image from "next/image"
import Link from "next/link"
import { HeroGrid } from "@/components/hero-grid"

const geist = Geist({
  subsets: ["latin"],
})

export const metadata: Metadata = {
  title: {
    absolute: "onlydiffs",
  },
  description:
    "The editor is over. The chat box was never it. See what your agent changed. Talk it through.",
}

export default function Home() {
  return (
    <div
      className={`${geist.className} relative flex min-h-svh flex-col items-center justify-center overflow-hidden bg-neutral-900 px-6`}
    >
      <HeroGrid />
      <section className="relative flex flex-col items-center text-center text-white">
        <Image src="/logo.png" alt="" width={80} height={80} priority className="size-20" />
        <h1 className="mt-8 font-medium font-mono text-3xl tracking-tight">onlydiffs</h1>
        <p className="mt-3 max-w-md text-base/7 text-white/70">
          The editor is over. The chat box was never it.
          <br />
          See what your agent changed. Talk it through.
        </p>
        <Link
          href="/docs"
          className="mt-10 text-sm text-white/70 transition-colors duration-150 hover:text-white"
        >
          Docs
        </Link>
      </section>
    </div>
  )
}
