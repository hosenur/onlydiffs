"use client"

import { useEffect, useMemo, useState } from "react"
import {
  Snippet,
  SnippetTab,
  SnippetTabPanel,
  SnippetTabPanels,
  SnippetTabsList,
} from "@/components/ui/snippet"

interface CommandItem {
  id: "npm" | "pnpm" | "yarn" | "bun"
  command: string
}

interface PackageCommandProps {
  command: string
  mode?: "auto" | "install" | "exec"
}

type PackageManager = "npm" | "pnpm" | "yarn" | "bun"

const PM_STORAGE_KEY = "preferred-package-manager"
const PM_EVENT = "package-manager-change"

function getStoredPackageManager(): PackageManager {
  if (typeof window === "undefined") return "npm"
  const value = localStorage.getItem(PM_STORAGE_KEY)
  if (value === "npm" || value === "pnpm" || value === "yarn" || value === "bun") return value
  return "npm"
}

function setStoredPackageManager(packageManager: PackageManager) {
  if (typeof window === "undefined") return
  localStorage.setItem(PM_STORAGE_KEY, packageManager)
  window.dispatchEvent(new CustomEvent(PM_EVENT, { detail: packageManager }))
}

function parseCommand(
  input: string,
  mode: PackageCommandProps["mode"],
): { mode: "install" | "exec"; args: string } {
  const command = input.trim().replace(/\s+/g, " ")

  if (mode === "install") {
    return {
      mode: "install",
      args: command.replace(/^(npm\s+(?:i|install)|pnpm\s+add|yarn\s+add|bun\s+add)\s+/, ""),
    }
  }

  if (mode === "exec") {
    return {
      mode: "exec",
      args: command.replace(/^(npx|pnpm\s+dlx|yarn\s+dlx|bunx)\s+/, ""),
    }
  }

  if (/^(npm\s+(?:i|install)|pnpm\s+add|yarn\s+add|bun\s+add)\s+/.test(command)) {
    return {
      mode: "install",
      args: command.replace(/^(npm\s+(?:i|install)|pnpm\s+add|yarn\s+add|bun\s+add)\s+/, ""),
    }
  }

  if (/^(npx|pnpm\s+dlx|yarn\s+dlx|bunx)\s+/.test(command)) {
    return {
      mode: "exec",
      args: command.replace(/^(npx|pnpm\s+dlx|yarn\s+dlx|bunx)\s+/, ""),
    }
  }

  if (
    /(^|\/)shadcn(@|$)/.test(command) ||
    /\badd\s+@/.test(command) ||
    /\binit\b/.test(command) ||
    /@latest\b/.test(command)
  ) {
    return { mode: "exec", args: command }
  }

  return { mode: "install", args: command }
}

function buildItems(parsed: { mode: "install" | "exec"; args: string }): CommandItem[] {
  const { mode, args } = parsed

  if (mode === "exec") {
    return [
      { id: "npm", command: `npx ${args}` },
      { id: "pnpm", command: `pnpm dlx ${args}` },
      { id: "yarn", command: `yarn dlx ${args}` },
      { id: "bun", command: `bunx ${args}` },
    ]
  }

  return [
    { id: "npm", command: `npm install ${args}` },
    { id: "pnpm", command: `pnpm add ${args}` },
    { id: "yarn", command: `yarn add ${args}` },
    { id: "bun", command: `bun add ${args}` },
  ]
}

export function PackageCommand({ command, mode = "auto" }: PackageCommandProps) {
  const parsed = useMemo(() => parseCommand(command, mode), [command, mode])
  const items = useMemo(() => buildItems(parsed), [parsed])
  const [selectedPackageManager, setSelectedPackageManager] = useState<PackageManager>("npm")

  useEffect(() => {
    setSelectedPackageManager(getStoredPackageManager())

    const handlePackageManagerChange = (event: Event) => {
      setSelectedPackageManager((event as CustomEvent<PackageManager>).detail)
    }

    window.addEventListener(PM_EVENT, handlePackageManagerChange)
    return () => window.removeEventListener(PM_EVENT, handlePackageManagerChange)
  }, [])

  const handleSelectionChange = (key: string | number) => {
    const packageManager = String(key) as PackageManager
    setSelectedPackageManager(packageManager)
    setStoredPackageManager(packageManager)
  }

  return (
    <Snippet
      className="not-typeset mt-6 border"
      selectedKey={selectedPackageManager}
      onSelectionChange={handleSelectionChange}
    >
      <SnippetTabsList className="bg-shiki" items={items}>
        {(item) => (
          <SnippetTab id={item.id} key={item.id}>
            {item.id}
          </SnippetTab>
        )}
      </SnippetTabsList>
      <SnippetTabPanels items={items}>
        {(item) => (
          <SnippetTabPanel className="bg-shiki" id={item.id}>
            {item.command}
          </SnippetTabPanel>
        )}
      </SnippetTabPanels>
    </Snippet>
  )
}
