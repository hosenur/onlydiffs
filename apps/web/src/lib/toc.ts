import GithubSlugger from "github-slugger"
import type { TocItem } from "@/types/content"

function stripInlineMarkdown(value: string) {
  return value
    .replace(/`([^`]+)`/g, "$1")
    .replace(/!?(?:\[([^\]]+)\])\([^)]*\)/g, "$1")
    .replace(/[*_~]/g, "")
    .trim()
}

export function extractToc(content: string): TocItem[] {
  const slugger = new GithubSlugger()
  const withoutCodeFences = content.replace(/```[\s\S]*?```/g, "")
  const items: TocItem[] = []

  for (const match of withoutCodeFences.matchAll(/^(#{2,4})\s+(.+?)\s*#*$/gm)) {
    const title = stripInlineMarkdown(match[2])

    items.push({
      title,
      url: `#${slugger.slug(title)}`,
      depth: match[1].length,
    })
  }

  return items
}

export function stripFrontmatter(value: string) {
  return value.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, "").trim()
}
