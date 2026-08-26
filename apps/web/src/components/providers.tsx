"use client"

import { ThemeProvider } from "next-themes"
import { I18nProvider } from "react-aria-components/I18nProvider"

interface ProvidersProps {
  children: React.ReactNode
  lang?: string
}

export function Providers({ children, lang }: ProvidersProps) {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <I18nProvider locale={lang}>{children}</I18nProvider>
    </ThemeProvider>
  )
}
