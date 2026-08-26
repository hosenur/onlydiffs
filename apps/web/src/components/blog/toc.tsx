"use client"

import { LayoutGroup, motion } from "motion/react"
import { useEffect, useId, useMemo, useRef, useState } from "react"
import scrollIntoView from "scroll-into-view-if-needed"
import { twJoin } from "tailwind-merge"
import type { TocItem } from "@/types/content"

function getHash(url: string) {
  return decodeURIComponent(url.replace(/^#/, ""))
}

export function BlogToc({ items }: { items: TocItem[] }) {
  const layoutId = useId()
  const ids = useMemo(() => items.map((item) => getHash(item.url)), [items])
  const [activeId, setActiveId] = useState(() => ids[0] ?? "")
  const navRef = useRef<HTMLElement>(null)
  const activeItemRef = useRef<HTMLAnchorElement>(null)

  useEffect(() => {
    if (ids.length === 0) return

    const updateFromHash = () => {
      const id = decodeURIComponent(window.location.hash.replace(/^#/, ""))

      if (id && ids.includes(id)) setActiveId(id)
    }

    updateFromHash()

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0]

        if (visible) setActiveId(visible.target.id)
      },
      {
        rootMargin: "-96px 0px -65% 0px",
        threshold: [0, 1],
      },
    )

    for (const id of ids) {
      const heading = document.getElementById(id)

      if (heading) observer.observe(heading)
    }

    window.addEventListener("hashchange", updateFromHash)

    return () => {
      observer.disconnect()
      window.removeEventListener("hashchange", updateFromHash)
    }
  }, [ids])

  useEffect(() => {
    if (!activeItemRef.current || !navRef.current) return

    scrollIntoView(activeItemRef.current, {
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
      block: "center",
      boundary: navRef.current,
      inline: "nearest",
      scrollMode: "if-needed",
    })
  }, [activeId])

  if (items.length === 0) return null

  return (
    <LayoutGroup id={layoutId}>
      <aside
        className="sticky top-6 hidden w-80 shrink-0 lg:block lg:py-20 xl:w-96"
        aria-label="Table of contents"
      >
        <nav
          ref={navRef}
          className="scrollbar-thin scroll-fade-y relative max-h-[calc(100vh-12rem)] overflow-y-auto"
        >
          <ul className="text-sm/7">
            {items.map((item) => {
              const id = getHash(item.url)
              const active = id === activeId

              return (
                <li key={item.url}>
                  <a
                    ref={active ? activeItemRef : undefined}
                    href={item.url}
                    aria-current={active ? "location" : undefined}
                    className={twJoin(
                      "relative block gap-2 py-2 pr-4 transition-colors",
                      active ? "text-fg" : "text-muted-fg hover:text-fg",
                    )}
                    style={{ paddingLeft: `${56 + Math.max(0, item.depth - 2) * 12}px` }}
                  >
                    <span>{item.title}</span>
                    {active ? (
                      <motion.span
                        layoutId="activeBlogTocItem"
                        className="absolute inset-s-0 inset-y-2 w-0.5 bg-fg"
                        transition={{ type: "spring", stiffness: 450, damping: 38, mass: 0.8 }}
                      />
                    ) : null}
                  </a>
                </li>
              )
            })}
          </ul>
        </nav>
      </aside>
    </LayoutGroup>
  )
}
