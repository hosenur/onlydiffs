import type { MetadataRoute } from "next"
import { app } from "@/config/app"
import { getBlogImage, getBlogPosts, getBlogSlug } from "@/lib/blog"
import { source } from "@/lib/source"

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const posts = getBlogPosts()
  const images = await Promise.all(posts.map(getBlogImage))

  const docs: MetadataRoute.Sitemap = source.getPages().map((page) => ({
    url: `${app.url}${page.url}`,
  }))

  const blogPosts: MetadataRoute.Sitemap = posts.map((post, index) => ({
    url: `${app.url}/blog/${getBlogSlug(post)}`,
    lastModified: post.date,
    images: [new URL(images[index].src.src, app.url).toString()],
  }))

  return [...docs, { url: `${app.url}/blog` }, ...blogPosts]
}
