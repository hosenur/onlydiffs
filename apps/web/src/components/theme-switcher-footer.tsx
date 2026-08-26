"use client"

import { ComputerDesktopIcon, MoonIcon, SunIcon } from "@heroicons/react/24/outline"
import { useTheme } from "next-themes"
import { useEffect, useState } from "react"
import { ToggleGroup, ToggleGroupItem } from "@onlydiffs/ui/toggle-group"

const themeOptions = [
  { value: "light", icon: SunIcon, label: "Light theme" },
  { value: "dark", icon: MoonIcon, label: "Dark theme" },
  { value: "system", icon: ComputerDesktopIcon, label: "System theme" },
] as const

type ThemeValue = (typeof themeOptions)[number]["value"]

function isThemeValue(value: unknown): value is ThemeValue {
  return themeOptions.some((option) => option.value === value)
}

export function ThemeSwitcherFooter() {
  const [mounted, setMounted] = useState(false)
  const { theme, setTheme } = useTheme()
  const currentTheme = isThemeValue(theme) ? theme : "system"

  useEffect(() => {
    setMounted(true)
  }, [])

  return (
    <ToggleGroup
      aria-label="Choose theme"
      className="rounded-full [--toggle-selected-bg:var(--color-secondary)] [--toggle-selected-fg:var(--color-secondary-fg)] *:data-[slot=toggle-group-item]:rounded-full"
      disallowEmptySelection
      onSelectionChange={(keys) => {
        const selectedTheme = [...keys][0]

        if (isThemeValue(selectedTheme)) {
          setTheme(selectedTheme)
        }
      }}
      selectedKeys={mounted ? new Set([currentTheme]) : new Set()}
      selectionMode="single"
      size="sq-sm"
    >
      {themeOptions.map((option) => (
        <ToggleGroupItem key={option.value} id={option.value} aria-label={option.label}>
          <option.icon />
        </ToggleGroupItem>
      ))}
    </ToggleGroup>
  )
}
