import type { MDXComponents } from "mdx/types"

export interface TocItem {
  title: string
  url: string
  depth: number
}

export interface PageTreePage {
  type: "page"
  name: React.ReactNode
  url: string
  status?: string
}

export interface PageTreeFolder {
  type: "folder"
  name: React.ReactNode
  index?: PageTreePage
  children: PageTreeNode[]
}

export interface PageTreeSeparator {
  type: "separator"
  name: React.ReactNode
}

export type PageTreeNode = PageTreePage | PageTreeFolder | PageTreeSeparator

export interface PageTreeRoot {
  name: React.ReactNode
  children: PageTreeNode[]
}

export interface MdxModule {
  default: React.ComponentType<{ components?: MDXComponents }>
}
