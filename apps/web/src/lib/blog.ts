import "server-only"

import { readdirSync, readFileSync } from "node:fs"
import path from "node:path"
import matter from "gray-matter"
import type { StaticImageData } from "next/image"
import { extractToc } from "@/lib/toc"
import type { MdxModule, TocItem } from "@/types/content"

const blogDirectory = path.join(process.cwd(), "src/content/blog")

export interface BlogPost {
  info: {
    path: string
  }
  title: string
  description: string
  date: string
  author: {
    name: string
    username: string
  }
  toc: TocItem[]
  raw: string
  image: {
    src: string
    alt: string
    load: () => Promise<{ default: StaticImageData }>
  }
  load: () => Promise<MdxModule>
}

function getBlogFiles() {
  return readdirSync(blogDirectory, { withFileTypes: true }).flatMap((entry) => {
    if (!entry.isDirectory()) return []

    const filePath = `${entry.name}/index.mdx`
    return [filePath]
  })
}

const posts = getBlogFiles().map<BlogPost>((filePath) => {
  const source = readFileSync(path.join(blogDirectory, filePath), "utf8")
  const { data, content } = matter(source)

  if (
    typeof data.title !== "string" ||
    typeof data.description !== "string" ||
    typeof data.date !== "string" ||
    typeof data.author?.name !== "string" ||
    typeof data.author?.username !== "string" ||
    typeof data.image?.src !== "string" ||
    typeof data.image?.alt !== "string"
  ) {
    throw new Error(`Invalid blog frontmatter in ${filePath}`)
  }

  const imagePath = `${path.posix.dirname(filePath)}/${data.image.src.replace(/^\.\//, "")}`

  return {
    info: { path: filePath },
    title: data.title,
    description: data.description,
    date: data.date,
    author: {
      name: data.author.name,
      username: data.author.username,
    },
    toc: extractToc(content),
    raw: source,
    image: {
      src: data.image.src,
      alt: data.image.alt,
      load: () => import(`../content/blog/${imagePath}`),
    },
    load: () => import(`../content/blog/${filePath}`) as Promise<MdxModule>,
  }
})

export function getBlogSlug(post: BlogPost) {
  const slug = post.info.path.replace(/\.mdx?$/, "")

  if (slug.endsWith("/index")) {
    return slug.slice(0, -"/index".length)
  }

  return slug
}

export async function getBlogImage(post: BlogPost) {
  const { default: image } = await post.image.load()

  return {
    src: image,
    alt: post.image.alt,
  }
}

export function getBlogPosts() {
  return [...posts].sort((a, b) => b.date.localeCompare(a.date))
}

export function getBlogPost(slug: string) {
  return getBlogPosts().find((post) => getBlogSlug(post) === slug)
}

export function getReadingTime(post: BlogPost) {
  const words = post.raw
    .replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, "")
    .replace(/```[\s\S]*?```/g, "")
    .replace(/`[^`]*`/g, "")
    .replace(/<[^>]+>/g, "")
    .replace(/!?(?:\[([^\]]*)\])\([^)]*\)/g, "$1")
    .trim()
    .split(/\s+/)
    .filter(Boolean).length

  return Math.max(1, Math.ceil(words / 200))
}

export function formatDate(value: string) {
  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeZone: "UTC",
  }).format(new Date(value))
}
