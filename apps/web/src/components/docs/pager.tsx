import Link from "next/link"
import { source } from "@/lib/source"
import type { PageTreeNode, PageTreePage } from "@/types/content"

interface DocsPagerProps {
  url: string
}

function flattenPages(nodes: PageTreeNode[]): PageTreePage[] {
  return nodes.flatMap((node) => {
    if (node.type === "separator") return []
    if (node.type === "page") return [node]

    return [...(node.index ? [node.index] : []), ...flattenPages(node.children)]
  })
}

export function DocsPager({ url }: DocsPagerProps) {
  const pages = flattenPages(source.getPageTree().children)
  const index = pages.findIndex((page) => page.url === url)
  const previous = index > 0 ? pages[index - 1] : undefined
  const next = index >= 0 ? pages[index + 1] : undefined

  if (!previous && !next) {
    return null
  }

  return (
    <nav className="mt-8 flex justify-between gap-x-3">
      {previous ? (
        <Link className="group flex items-center gap-1 sm:text-sm" href={previous.url}>
          <span className="group-hover:-translate-x-2 -translate-x-1 transition-transform">
            &larr;
          </span>
          <span className="sr-only">Previous</span>
          {previous.name}
        </Link>
      ) : (
        <div />
      )}
      {next ? (
        <Link className="group flex items-center gap-1 sm:text-sm" href={next.url}>
          <span className="sr-only">Next</span>
          {next.name}
          <span className="translate-x-1 transition-transform group-hover:translate-x-2">
            &rarr;
          </span>
        </Link>
      ) : null}
    </nav>
  )
}
