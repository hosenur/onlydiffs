import type { PageTreeNode } from "@/types/content"

export interface NavigationItem {
  label: React.ReactNode
  textValue: string
  href: string
}

export function flattenSidebar(nodes: PageTreeNode[]) {
  const items: NavigationItem[] = []

  for (const node of nodes) {
    if (node.type === "separator") {
      continue
    }

    if (node.type === "folder") {
      if (node.index) {
        items.push({
          label: node.name,
          textValue: typeof node.name === "string" ? node.name : node.index.url,
          href: node.index.url,
        })
      }

      items.push(...flattenSidebar(node.children))
      continue
    }

    items.push({
      label: node.name,
      textValue: typeof node.name === "string" ? node.name : node.url,
      href: node.url,
    })
  }

  return items
}

export function normalizePath(value: string) {
  if (value.length > 1 && value.endsWith("/")) {
    return value.slice(0, -1)
  }

  return value
}

export const themes = {
  light: "light-plus",
  dark: "nord",
}
