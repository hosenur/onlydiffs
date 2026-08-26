import "server-only"

import { readdirSync, readFileSync } from "node:fs"
import path from "node:path"
import matter from "gray-matter"
import { extractToc } from "@/lib/toc"
import type {
  MdxModule,
  PageTreeFolder,
  PageTreeNode,
  PageTreePage,
  PageTreeRoot,
  TocItem,
} from "@/types/content"

const docsDirectory = path.join(process.cwd(), "src/content/docs")

interface DocMetadata {
  title: string
  description?: string
  status?: string
}

export interface DocPage {
  path: string
  slugs: string[]
  url: string
  data: DocMetadata & {
    toc: TocItem[]
    raw: string
  }
  load: () => Promise<MdxModule>
}

interface MetaFile {
  title?: string
  pages?: string[]
}

function titleFromSegment(value: string) {
  return value
    .split("-")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ")
}

function readMeta(directory: string): MetaFile {
  try {
    return JSON.parse(readFileSync(path.join(directory, "meta.json"), "utf8")) as MetaFile
  } catch {
    return {}
  }
}

function getMdxFiles(directory: string, prefix = ""): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const relativePath = prefix ? `${prefix}/${entry.name}` : entry.name
    const absolutePath = path.join(directory, entry.name)

    if (entry.isDirectory()) {
      return getMdxFiles(absolutePath, relativePath)
    }

    return entry.isFile() && entry.name.endsWith(".mdx") ? [relativePath] : []
  })
}

function pathToSlugs(filePath: string) {
  const withoutExtension = filePath.replace(/\.mdx$/, "")
  const segments = withoutExtension.split("/")

  return segments.at(-1) === "index" ? segments.slice(0, -1) : segments
}

function createPages() {
  return getMdxFiles(docsDirectory).map<DocPage>((filePath) => {
    const absolutePath = path.join(docsDirectory, filePath)
    const source = readFileSync(absolutePath, "utf8")
    const { data, content } = matter(source)
    const slugs = pathToSlugs(filePath)
    const url = slugs.length > 0 ? `/docs/${slugs.join("/")}` : "/docs"

    if (typeof data.title !== "string") {
      throw new Error(`Missing title in ${filePath}`)
    }

    return {
      path: filePath,
      slugs,
      url,
      data: {
        title: data.title,
        description: typeof data.description === "string" ? data.description : undefined,
        status: typeof data.status === "string" ? data.status : undefined,
        toc: extractToc(content),
        raw: source,
      },
      load: () => import(`../content/docs/${filePath}`) as Promise<MdxModule>,
    }
  })
}

function compareByOrder(left: string, right: string, order: string[]) {
  const leftIndex = order.indexOf(left)
  const rightIndex = order.indexOf(right)

  if (leftIndex === -1 && rightIndex === -1) return left.localeCompare(right)
  if (leftIndex === -1) return 1
  if (rightIndex === -1) return -1
  return leftIndex - rightIndex
}

function buildFolder(
  directory: string,
  segments: string[],
  pagesBySlug: Map<string, DocPage>,
): PageTreeFolder {
  const meta = readMeta(directory)
  const entries = readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isDirectory() || entry.name.endsWith(".mdx"))
    .sort((left, right) => {
      const leftKey = left.isDirectory() ? left.name : left.name.replace(/\.mdx$/, "")
      const rightKey = right.isDirectory() ? right.name : right.name.replace(/\.mdx$/, "")
      return compareByOrder(leftKey, rightKey, meta.pages ?? [])
    })
  const children: PageTreeNode[] = []
  let index: PageTreePage | undefined

  for (const entry of entries) {
    if (entry.isDirectory()) {
      children.push(
        buildFolder(path.join(directory, entry.name), [...segments, entry.name], pagesBySlug),
      )
      continue
    }

    const fileName = entry.name.replace(/\.mdx$/, "")
    const slugs = fileName === "index" ? segments : [...segments, fileName]
    const page = pagesBySlug.get(slugs.join("/"))

    if (!page) continue

    const node: PageTreePage = {
      type: "page",
      name: page.data.title,
      url: page.url,
      status: page.data.status,
    }

    if (fileName === "index") {
      index = node
    } else {
      children.push(node)
    }
  }

  return {
    type: "folder",
    name: meta.title ?? titleFromSegment(segments.at(-1) ?? "Docs"),
    index,
    children,
  }
}

function buildPageTree(pagesBySlug: Map<string, DocPage>): PageTreeRoot {
  const folder = buildFolder(docsDirectory, [], pagesBySlug)
  const children = folder.index ? [folder.index, ...folder.children] : folder.children

  return {
    name: folder.name,
    children,
  }
}

interface ContentIndex {
  pages: DocPage[]
  pagesBySlug: Map<string, DocPage>
  pageTree: PageTreeRoot
}

let productionIndex: ContentIndex | undefined

function createContentIndex(): ContentIndex {
  const pages = createPages()
  const pagesBySlug = new Map(pages.map((page) => [page.slugs.join("/"), page]))

  return {
    pages,
    pagesBySlug,
    pageTree: buildPageTree(pagesBySlug),
  }
}

function getContentIndex() {
  if (process.env.NODE_ENV !== "production") {
    return createContentIndex()
  }

  productionIndex ??= createContentIndex()
  return productionIndex
}

export const source = {
  getPage(slugs: string[] | undefined) {
    return getContentIndex().pagesBySlug.get((slugs ?? []).join("/"))
  },
  getPages() {
    return getContentIndex().pages
  },
  getPageTree() {
    return getContentIndex().pageTree
  },
  generateParams() {
    return getContentIndex().pages.map((page) => ({ slug: page.slugs }))
  },
}
