import type { Metadata } from "next"
import { Geist } from "next/font/google"
import Image from "next/image"
import "drawably/font.css"
import { HeroGrid } from "@/components/hero-grid"
import { HomeDocsButton, HomeTagline } from "@/components/home-tagline"

const geist = Geist({
  subsets: ["latin"],
})

export const metadata: Metadata = {
  title: {
    absolute: "onlydiffs",
  },
  description:
    "A desktop app for reviewing every change your coding agent makes. Inspect diffs and send line-level feedback straight to Claude.",
}

export default function Home() {
  return (
    <div
      className={`${geist.className} relative flex min-h-svh flex-col items-center justify-center overflow-hidden bg-neutral-900 px-6`}
    >
      <HeroGrid />
      <section className="relative flex flex-col items-center text-center text-white">
        <Image src="/logo.png" alt="" width={80} height={80} priority className="size-20" />
        <h1 className="mt-8 font-['Drawably_Pen'] text-3xl tracking-tight">onlydiffs</h1>
        <HomeTagline />
        <form action="/docs" className="mt-10">
          <HomeDocsButton />
        </form>
      </section>
    </div>
  )
}
