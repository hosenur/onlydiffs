"use client"

import { ComputerDesktopIcon, MoonIcon, SunIcon } from "@heroicons/react/24/outline"
import { useTheme } from "next-themes"
import { useEffect, useState } from "react"
import { ToggleButton } from "react-aria-components"
import { buttonStyles } from "@onlydiffs/ui/button"

const themes = ["system", "light", "dark"] as const

export function ThemeSwitcher() {
  const [mounted, setMounted] = useState(false)
  const { theme, setTheme } = useTheme()

  useEffect(() => {
    setMounted(true)
  }, [])

  const currentTheme = themes.includes(theme as (typeof themes)[number])
    ? (theme as (typeof themes)[number])
    : "system"

  const Icon =
    currentTheme === "system" ? ComputerDesktopIcon : currentTheme === "dark" ? MoonIcon : SunIcon

  const toggleTheme = () => {
    const index = themes.indexOf(currentTheme)
    setTheme(themes[(index + 1) % themes.length])
  }

  return (
    <ToggleButton
      className={buttonStyles({ intent: "plain", size: "sq-sm", isCircle: true })}
      onPress={toggleTheme}
      aria-label="Toggle theme"
    >
      {mounted && <Icon />}
    </ToggleButton>
  )
}
