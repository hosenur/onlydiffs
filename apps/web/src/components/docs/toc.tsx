"use client"

import { Bars3BottomLeftIcon } from "@heroicons/react/24/outline"
import { LayoutGroup, motion } from "motion/react"
import { useEffect, useId, useMemo, useRef, useState } from "react"
import scrollIntoView from "scroll-into-view-if-needed"
import { twJoin } from "tailwind-merge"
import { DocsPageActions } from "@/components/docs/page-actions"
import type { TocItem } from "@/types/content"

function getHash(url: string) {
  return decodeURIComponent(url.replace(/^#/, ""))
}

interface DocsTocProps {
  items: TocItem[]
  copyContent?: string
  url?: string
}

export function DocsToc({ items, copyContent, url }: DocsTocProps) {
  const id = useId()
  const ids = useMemo(() => items.map((item) => getHash(item.url)), [items])
  const [activeId, setActiveId] = useState(() => ids[0] ?? "")
  const tocRef = useRef<HTMLElement>(null)
  const activeItemRef = useRef<HTMLAnchorElement>(null)

  useEffect(() => {
    if (ids.length === 0) {
      return
    }

    const updateFromHash = () => {
      const id = decodeURIComponent(window.location.hash.replace(/^#/, ""))

      if (id && ids.includes(id)) {
        setActiveId(id)
      }
    }

    updateFromHash()

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0]

        if (visible) {
          setActiveId(visible.target.id)
        }
      },
      {
        rootMargin: "-96px 0px -65% 0px",
        threshold: [0, 1],
      },
    )

    for (const id of ids) {
      const element = document.getElementById(id)

      if (element) {
        observer.observe(element)
      }
    }

    window.addEventListener("hashchange", updateFromHash)

    return () => {
      observer.disconnect()
      window.removeEventListener("hashchange", updateFromHash)
    }
  }, [ids])

  useEffect(() => {
    const activeItem = activeItemRef.current
    const toc = tocRef.current

    if (!activeItem || !toc) return

    scrollIntoView(activeItem, {
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
      block: "center",
      boundary: toc,
      inline: "nearest",
      scrollMode: "always",
    })
  }, [activeId])

  if (items.length === 0) {
    return null
  }

  return (
    <LayoutGroup id={id}>
      <aside className="hidden xl:block">
        <div className="sticky top-30 flex max-h-[calc(100vh-9rem)] flex-col">
          {copyContent && url ? <DocsPageActions content={copyContent} url={url} /> : null}
          <p
            className={twJoin(
              "mb-3 flex items-center gap-x-2 font-medium text-muted-fg text-xs uppercase",
              copyContent && url ? "mt-6" : "mt-0",
            )}
          >
            <Bars3BottomLeftIcon className="size-4" />
            On this page
          </p>
          <nav
            ref={tocRef}
            className="scrollbar-thin scroll-fade-y relative min-h-0 space-y-1 overflow-y-auto pl-6"
          >
            {items.map((item) => {
              const id = getHash(item.url)
              const active = id === activeId

              return (
                <a
                  ref={active ? activeItemRef : undefined}
                  key={item.url}
                  href={item.url}
                  aria-current={active ? "location" : undefined}
                  className={twJoin(
                    "relative block py-1 text-sm/6",
                    active ? "text-primary-subtle-fg" : "text-muted-fg hover:text-fg",
                  )}
                  style={{
                    paddingLeft: `${Math.max(0, item.depth - 2) * 12}px`,
                  }}
                >
                  {item.title}
                  {active && (
                    <motion.span
                      transition={{
                        type: "spring",
                        stiffness: 450,
                        damping: 35,
                        mass: 0.8,
                      }}
                      layoutId="currentIndicator"
                      className="-left-5 -translate-y-1/2 absolute top-1/2 hidden size-1.5 rounded-full bg-primary md:block dark:bg-primary-subtle-fg"
                    />
                  )}
                </a>
              )
            })}
          </nav>
        </div>
      </aside>
    </LayoutGroup>
  )
}
