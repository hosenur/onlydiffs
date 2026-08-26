import { ImageResponse } from "next/og"
import { Logo } from "@/components/logo"

async function loadAssets(): Promise<
  { name: string; data: Buffer; weight: 400 | 500; style: "normal" }[]
> {
  const [{ base64Font: normal }, { base64Font: semibold }] = await Promise.all([
    import("./inter-regular.json").then((mod) => mod.default || mod),
    import("./inter-semibold.json").then((mod) => mod.default || mod),
  ])

  return [
    {
      name: "Inter",
      data: Buffer.from(normal, "base64"),
      weight: 400 as const,
      style: "normal" as const,
    },
    {
      name: "Inter",
      data: Buffer.from(semibold, "base64"),
      weight: 500 as const,
      style: "normal" as const,
    },
  ]
}

export async function GET(request: Request) {
  const { searchParams } = new URL(request.url)
  const title = searchParams.get("title")
  const description = searchParams.get("description")

  const fonts = await loadAssets()

  return new ImageResponse(
    <div
      tw="flex h-full w-full bg-zinc-900 text-white"
      style={{
        fontFamily: "Inter",
        // backgroundImage: `url(${new URL('/images/og-background.png', process.env.NEXT_PUBLIC_APP_URL).toString()})`,
      }}
    >
      <div tw="flex absolute flex-row top-0 left-34 top-32 text-white">
        <Logo
          style={{
            width: "32px",
            height: "32px",
          }}
        />
      </div>
      <div tw="flex flex-col justify-start items-start inset-34 mt-16">
        <div
          tw="tracking-tight leading-[1.5] mt-9 mb-6"
          style={{
            textWrap: "balance",
            fontWeight: 400,
            fontSize: 54,
            letterSpacing: "-0.04em",
          }}
        >
          {title}
        </div>
        <div
          tw="max-w-4xl text-white/80"
          style={{
            lineHeight: 1.7,
            fontSize: 33,
            textWrap: "balance",
            fontWeight: 400,
            letterSpacing: "-0.02em",
          }}
        >
          {description}
        </div>
      </div>
    </div>,
    {
      width: 1200,
      height: 630,
      fonts,
    },
  )
}
