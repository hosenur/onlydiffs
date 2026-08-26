import type { Metadata } from "next"
import Image from "next/image"
import { notFound } from "next/navigation"
import { BlogShareActions } from "@/components/blog/share-actions"
import { BlogToc } from "@/components/blog/toc"
import { JsonLd } from "@/components/seo/json-ld"
import { Avatar } from "@onlydiffs/ui/avatar"
import { Container } from "@onlydiffs/ui/container"
import { Strong, Text } from "@/components/ui/text"
import { app } from "@/config/app"
import {
  formatDate,
  getBlogImage,
  getBlogPost,
  getBlogPosts,
  getBlogSlug,
  getReadingTime,
} from "@/lib/blog"
import { getMDXComponents } from "@/mdx-components"

type BlogPostPageProps = {
  params: Promise<{
    slug: string
  }>
}

export function generateStaticParams() {
  return getBlogPosts().map((post) => ({
    slug: getBlogSlug(post),
  }))
}

export async function generateMetadata({ params }: BlogPostPageProps): Promise<Metadata> {
  const { slug } = await params
  const post = getBlogPost(slug)

  if (!post) {
    return {}
  }

  const image = await getBlogImage(post)
  const postUrl = `${app.url}/blog/${getBlogSlug(post)}`
  const authorUrl = `https://github.com/${post.author.username}`
  const imageUrl = new URL(image.src.src, app.url).toString()

  return {
    title: post.title,
    description: post.description,
    alternates: {
      canonical: postUrl,
    },
    authors: [
      {
        name: post.author.name,
        url: authorUrl,
      },
    ],
    creator: post.author.name,
    publisher: app.name,
    openGraph: {
      type: "article",
      title: post.title,
      description: post.description,
      url: postUrl,
      siteName: app.name,
      locale: "en_US",
      publishedTime: new Date(post.date).toISOString(),
      authors: [authorUrl],
      images: [
        {
          url: imageUrl,
          width: image.src.width,
          height: image.src.height,
          alt: image.alt,
        },
      ],
    },
    twitter: {
      card: "summary_large_image",
      title: post.title,
      description: post.description,
      images: [
        {
          url: imageUrl,
          alt: image.alt,
        },
      ],
    },
  }
}

export default async function BlogPostPage({ params }: BlogPostPageProps) {
  const { slug } = await params
  const post = getBlogPost(slug)

  if (!post) {
    notFound()
  }

  const [{ default: MDX }, image] = await Promise.all([post.load(), getBlogImage(post)])
  const postUrl = `${app.url}/blog/${getBlogSlug(post)}`
  const authorUrl = `https://github.com/${post.author.username}`
  const imageUrl = new URL(image.src.src, app.url).toString()
  const publishedTime = new Date(post.date).toISOString()

  return (
    <div className="border-b [--horizontal-padding:--spacing(0)] lg:[--horizontal-padding:--spacing(16)]">
      <JsonLd
        data={{
          "@context": "https://schema.org",
          "@type": "BlogPosting",
          "@id": `${postUrl}#article`,
          mainEntityOfPage: {
            "@type": "WebPage",
            "@id": postUrl,
          },
          headline: post.title,
          description: post.description,
          image: {
            "@type": "ImageObject",
            url: imageUrl,
            width: image.src.width,
            height: image.src.height,
          },
          datePublished: publishedTime,
          author: {
            "@type": "Person",
            name: post.author.name,
            url: authorUrl,
            image: `https://github.com/${post.author.username}.png`,
          },
          publisher: {
            "@type": "Organization",
            name: app.name,
            url: app.url,
            logo: {
              "@type": "ImageObject",
              url: `${app.url}/icon1.png`,
            },
          },
          inLanguage: "en",
        }}
      />
      <Container>
        <div className="mt-4 flex flex-col-reverse gap-6 p-(--horizontal-padding) sm:mt-0 sm:flex-row lg:border-x">
          <div className="flex w-full flex-col justify-between gap-6">
            <h1 className="text-3xl/10 tracking-tight lg:text-5xl/14">{post.title}</h1>
            <Text>
              <time dateTime={post.date}>{formatDate(post.date)}</time>
              <span className="mx-3">&middot;</span>
              {getReadingTime(post)} min read
            </Text>
          </div>
          <div className="w-full shrink-0 lg:w-156">
            <Image
              src={image.src}
              alt={image.alt}
              priority
              className="aspect-video w-full rounded-lg object-cover"
            />
          </div>
        </div>
      </Container>
      <div className="lg:border-y">
        <Container>
          <div className="flex items-center justify-between py-6 lg:border-x lg:px-(--horizontal-padding)">
            <div className="flex items-center gap-x-3">
              <Avatar
                src={`https://github.com/${post.author.username}.png`}
                alt={post.author.username}
                size="md"
              />
              <Strong className="hidden lg:block">{post.author.name}</Strong>
            </div>

            <BlogShareActions title={post.title} url={postUrl} />
          </div>
        </Container>
      </div>
      <div>
        <Container>
          <div className="flex items-start lg:border-x">
            <BlogToc items={post.toc} />
            <div className="min-w-0 lg:border-l [&>*:first-child]:mt-0">
              <div className="p-(--horizontal-padding)">
                <div className="typeset typeset-article max-w-2xl">
                  <MDX components={getMDXComponents()} />
                </div>
              </div>
            </div>
          </div>
        </Container>
      </div>
    </div>
  )
}
