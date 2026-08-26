import type { Metadata, Viewport } from "next"
import { Geist_Mono } from "next/font/google"
import localFont from "next/font/local"

import "./styles/globals.css"
import { Providers } from "@/components/providers"
import { app } from "@/config/app"

export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "white" },
    { media: "(prefers-color-scheme: dark)", color: "black" },
  ],
  viewportFit: "cover",
  width: "device-width",
  initialScale: 1,
}

const inter = localFont({
  src: [
    {
      path: "./fonts/InterVariable.woff2",
      weight: "100 900",
      style: "normal",
    },
    {
      path: "./fonts/InterVariable-Italic.woff2",
      weight: "100 900",
      style: "italic",
    },
  ],
  variable: "--font-inter",
})

const geistMono = Geist_Mono({
  subsets: ["latin"],
  variable: "--font-geist-mono",
})

export const metadata: Metadata = {
  metadataBase: new URL(app.url),
  title: {
    default: app.name,
    template: `%s | ${app.name}`,
  },
  description: app.description,
  alternates: {
    canonical: "./",
  },
  keywords: ["React", "Next.js", "TypeScript", "Documentation", "Web Development"],
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html
      lang="en"
      className={`${inter.variable} ${geistMono.variable} h-full antialiased`}
      suppressHydrationWarning
      data-scroll-behavior="smooth"
    >
      <body className="flex min-h-full flex-col">
        <Providers lang="en">
          <main>{children}</main>
        </Providers>
      </body>
    </html>
  )
}
