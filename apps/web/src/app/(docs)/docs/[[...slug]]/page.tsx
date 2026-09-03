import type { Metadata } from "next"
import { notFound } from "next/navigation"
import { Feedback } from "@/components/docs/feedback"
import { DocsPager } from "@/components/docs/pager"
import { DocsToc } from "@/components/docs/toc"
import { Text } from "@/components/ui/text"
import { app } from "@/config/app"
import { ogImage } from "@/lib/og"
import { source } from "@/lib/source"
import { stripFrontmatter } from "@/lib/toc"
import { getMDXComponents } from "@/mdx-components"

type DocsPageProps = {
  params: Promise<{
    slug?: string[]
  }>
}

export function generateStaticParams() {
  return source.generateParams()
}

export async function generateMetadata({ params }: DocsPageProps): Promise<Metadata> {
  const { slug } = await params
  const page = source.getPage(slug)

  if (!page) {
    return {}
  }
  return {
    title: page.data.title,
    description: page.data.description,
    alternates: {
      canonical: `${app.url}${page.url}`,
    },
    openGraph: {
      title: `${page.data.title} / ${app.name}`,
      description: page.data.description,
      type: "article",
      url: `${app.url}${page.url}`,
      siteName: app.name,
      locale: "en_US",
      images: [
        { url: ogImage({ title: page.data.title, description: page.data.description ?? "" }) },
      ],
    },
    twitter: {
      card: "summary_large_image",
      title: `${page.data.title} / ${app.name}`,
      description: page.data.description,
      images: [
        { url: ogImage({ title: page.data.title, description: page.data.description ?? "" }) },
      ],
    },
  }
}

export default async function DocsPage({ params }: DocsPageProps) {
  const { slug } = await params
  const page = source.getPage(slug)

  if (!page) {
    notFound()
  }

  const { default: MDX } = await page.load()
  const copyContent = stripFrontmatter(page.data.raw)

  return (
    <div className="grid w-full min-w-0 grid-cols-1 gap-10 xl:grid-cols-[1fr_260px]">
      <article className="w-full min-w-0 lg:px-28 lg:pb-16">
        <div className="mb-6 flex flex-col">
          <h1 className="text-3xl/10 tracking-tight">{page.data.title}</h1>
          {page.data.description ? (
            <Text className="mt-2 max-w-2xl sm:text-lg/8">{page.data.description}</Text>
          ) : null}
        </div>
        <div className="typeset typeset-docs [&>*:first-child]:mt-0">
          <MDX components={getMDXComponents()} />
          <Feedback />
        </div>
        <DocsPager url={page.url} />
      </article>
      <DocsToc copyContent={copyContent} items={page.data.toc} url={page.url} />
    </div>
  )
}
