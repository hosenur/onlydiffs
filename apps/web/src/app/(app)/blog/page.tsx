import type { Metadata } from "next"
import Image from "next/image"
import Link from "next/link"
import { Container } from "@onlydiffs/ui/container"
import { Text } from "@/components/ui/text"
import { type BlogPost, formatDate, getBlogImage, getBlogPosts, getBlogSlug } from "@/lib/blog"

export const metadata: Metadata = {
  title: "Blog",
  description: "Articles and notes from the Gridlines documentation system.",
}

interface PostImageProps {
  image: Awaited<ReturnType<typeof getBlogImage>>
  priority?: boolean
}
function PostImage({ image, priority = false }: PostImageProps) {
  return (
    <div className="group/image relative isolate block overflow-hidden rounded-lg bg-secondary">
      <span
        className="absolute inset-0 rounded-[inherit] border border-fg/50 mix-blend-overlay"
        aria-hidden
      />
      <Image
        src={image.src}
        alt={image.alt}
        priority={priority}
        className="aspect-video w-full object-cover object-center"
      />
    </div>
  )
}

function PostMeta({ post }: { post: BlogPost }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <time
        dateTime={post.date}
        className="shrink-0 text-muted-fg text-xs/5 uppercase tracking-wide"
      >
        {formatDate(post.date)}
      </time>
    </div>
  )
}

export default async function BlogIndexPage() {
  const posts = getBlogPosts()
  const images = await Promise.all(posts.map(getBlogImage))
  const primaryPost = posts[0]
  const secondaryPosts = posts.slice(1, 3)
  const archivePosts = posts.slice(3)

  return (
    <div className="[--horizontal-padding:--spacing(4)] lg:[--horizontal-padding:--spacing(16)]">
      <div className="border-t lg:border-t-0">
        <Container>
          <div className="border-x">
            {primaryPost ? (
              <section aria-label="Featured articles" className="overflow-hidden">
                <div className="grid grid-cols-1 gap-px bg-border lg:grid-cols-[1fr_28rem] lg:grid-rows-2">
                  <Link
                    href={`/blog/${getBlogSlug(primaryPost)}`}
                    className="bg-bg hover:bg-muted lg:row-span-2"
                  >
                    <article className="relative bg-bg p-(--horizontal-padding)">
                      <PostImage image={images[0]} priority />
                      <div className="pt-6">
                        <PostMeta post={primaryPost} />
                        <h2 className="mt-3 max-w-2xl font-medium text-base/6 tracking-tight sm:text-3xl/9">
                          {primaryPost.title}
                        </h2>
                        <Text className="mt-3 hidden max-w-2xl text-base/7 sm:text-base/7 lg:block">
                          {primaryPost.description}
                        </Text>
                      </div>
                    </article>
                  </Link>
                  {secondaryPosts.map((post, index) => (
                    <Link
                      href={`/blog/${getBlogSlug(post)}`}
                      className="block bg-bg hover:bg-muted"
                      key={post.info.path}
                    >
                      <article className="relative overflow-hidden p-(--horizontal-padding)">
                        <PostImage image={images[index + 1]} />
                        <div className="pt-5">
                          <PostMeta post={post} />
                          <h2 className="mt-2 font-medium text-base/6">{post.title}</h2>
                        </div>
                      </article>
                    </Link>
                  ))}
                </div>
              </section>
            ) : null}
          </div>
        </Container>
      </div>

      <section aria-labelledby="latest-stories" className="border-y">
        <Container>
          {archivePosts.length > 0 ? (
            <div className="border-x">
              <div className="flex items-end justify-between gap-6 border-b px-(--horizontal-padding) py-3 pt-16 lg:pb-6">
                <h2 id="latest-stories" className="font-medium text-base tracking-tight sm:text-xl">
                  <div className="flex items-center gap-x-3">
                    <svg
                      className="size-5 fill-primary-subtle text-primary-subtle-fg"
                      xmlns="http://www.w3.org/2000/svg"
                      viewBox="0 0 24 24"
                      fill="none"
                    >
                      <path
                        d="M2.49012 13.0894L1.53906 13.3984M22.4623 6.60006L21.5112 6.90908M2.49012 6.90863L1.53906 6.59961M22.4623 13.398L21.5112 13.089"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                      <path
                        d="M11.9991 2.75C7.99456 2.75 4.74823 5.99633 4.74823 10.0009C4.74823 12.3433 5.85901 14.4264 7.58257 15.7519C7.78345 15.9064 7.99265 16.0506 8.20938 16.1838C8.53152 16.3816 8.74793 16.7224 8.74793 17.1005V18.9988C8.74793 20.7944 10.2035 22.25 11.9991 22.25C13.7947 22.25 15.2503 20.7944 15.2503 18.9988V17.1005C15.2503 16.7224 15.4667 16.3816 15.7888 16.1838C16.0056 16.0506 16.2148 15.9064 16.4157 15.7519C18.1392 14.4264 19.25 12.3433 19.25 10.0009C19.25 5.99633 16.0037 2.75 11.9991 2.75Z"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                      <path
                        d="M8.74792 17.75H15.2503"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                    Keep exploring
                  </div>
                </h2>
              </div>
              <ul className="flex flex-col divide-y overflow-hidden">
                {archivePosts.map((post, index) => (
                  <li key={index}>
                    <Link
                      className="block px-(--horizontal-padding) py-3 hover:bg-muted lg:py-6"
                      href={`/blog/${getBlogSlug(post)}`}
                    >
                      <h2 className="font-medium text-base/6 lg:text-lg/8">{post.title}</h2>
                      <div className="mt-3">
                        <PostMeta post={post} />
                      </div>
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </Container>
      </section>
    </div>
  )
}
