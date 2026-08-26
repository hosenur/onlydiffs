"use client"

import { BookOpenIcon, CubeIcon, NewspaperIcon } from "@heroicons/react/24/outline"
import NextLink from "next/link"
import { usePathname } from "next/navigation"

import { useEffect, useLayoutEffect, useMemo, useRef } from "react"
import { Header } from "react-aria-components/Header"
import {
  ListBox,
  ListBoxItem,
  type ListBoxItemProps,
  ListBoxSection,
} from "react-aria-components/ListBox"
import { twJoin } from "tailwind-merge"
import { DiscordIcon } from "@/components/icons/discord-icon"
import { GithubIcon } from "@/components/icons/github-icon"
import { Badge, type BadgeProps } from "@onlydiffs/ui/badge"
import type { PageTreeNode, PageTreeRoot } from "@/types/content"

interface SidebarItem {
  label: React.ReactNode
  textValue: string
  href: string
  status?: string
}

interface SidebarSection {
  id: string
  label: React.ReactNode
  textValue: string
  items: SidebarItem[]
}

function normalizePath(value: string) {
  if (value.length > 1 && value.endsWith("/")) {
    return value.slice(0, -1)
  }

  return value
}

function getTextValue(name: React.ReactNode, fallback: string) {
  return typeof name === "string" ? name : fallback
}

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  return (
    target.isContentEditable ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  )
}

function collectSidebarItems(nodes: PageTreeNode[]) {
  const items: SidebarItem[] = []

  for (const node of nodes) {
    if (node.type === "separator") {
      continue
    }

    if (node.type === "folder") {
      if (node.index) {
        items.push({
          label: node.name,
          textValue: getTextValue(node.name, node.index.url),
          href: node.index.url,
          status: node.index.status,
        })
      }

      items.push(...collectSidebarItems(node.children))
      continue
    }

    items.push({
      label: node.name,
      textValue: getTextValue(node.name, node.url),
      href: node.url,
      status: node.status,
    })
  }

  return items
}

function createSection(
  label: React.ReactNode,
  textValue: string,
  items: SidebarItem[],
  index: number,
): SidebarSection | null {
  if (items.length === 0) {
    return null
  }

  return {
    id: `section-${index}-${textValue.toLowerCase().replace(/\s+/g, "-")}`,
    label,
    textValue,
    items,
  }
}

function buildSidebarSections(nodes: PageTreeNode[]) {
  const sections: SidebarSection[] = []

  for (const node of nodes) {
    if (node.type !== "folder") {
      continue
    }

    const items = collectSidebarItems([node])
    const section = createSection(
      node.name,
      getTextValue(node.name, node.index?.url ?? `Section ${sections.length + 1}`),
      items,
      sections.length,
    )

    if (section) {
      sections.push(section)
    }
  }

  return sections
}

interface DocsSidebarProps {
  tree: PageTreeRoot
}

export function DocsSidebar({ tree }: DocsSidebarProps) {
  const pathname = normalizePath(usePathname())
  const sections = useMemo(() => buildSidebarSections(tree.children), [tree])
  const items = useMemo(() => sections.flatMap((section) => section.items), [sections])
  const selectedHref = items.find((item) => normalizePath(item.href) === pathname)?.href
  const searchInputRef = useRef<HTMLInputElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const previousPathnameRef = useRef<string | null>(null)

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.key !== "/" ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        isEditableTarget(event.target)
      ) {
        return
      }

      event.preventDefault()
      searchInputRef.current?.focus()
      searchInputRef.current?.select()
    }

    window.addEventListener("keydown", handleKeyDown)

    return () => {
      window.removeEventListener("keydown", handleKeyDown)
    }
  }, [])

  useLayoutEffect(() => {
    const previousPathname = previousPathnameRef.current
    const initialRender = previousPathname === null

    if (previousPathname === pathname) {
      return
    }

    previousPathnameRef.current = pathname

    const updateScrollPosition = () => {
      const container = listRef.current
      const activeItem = listRef.current?.querySelector<HTMLElement>('[data-docs-current="true"]')

      if (!container || !activeItem) {
        return
      }

      const containerRect = container.getBoundingClientRect()
      const itemRect = activeItem.getBoundingClientRect()
      const comfort = Math.min(140, container.clientHeight * 0.24)
      const insideComfortZone =
        itemRect.top >= containerRect.top + comfort &&
        itemRect.bottom <= containerRect.bottom - comfort

      if (insideComfortZone) {
        return
      }

      const itemTop = itemRect.top - containerRect.top + container.scrollTop
      const targetTop = itemTop + activeItem.offsetHeight / 2 - container.clientHeight * 0.35
      const maxScrollTop = container.scrollHeight - container.clientHeight

      container.scrollTo({
        top: Math.max(0, Math.min(targetTop, maxScrollTop)),
        behavior:
          initialRender || window.matchMedia("(prefers-reduced-motion: reduce)").matches
            ? "auto"
            : "smooth",
      })
    }

    if (initialRender) {
      updateScrollPosition()
      return
    }

    const frame = window.requestAnimationFrame(updateScrollPosition)

    return () => window.cancelAnimationFrame(frame)
  }, [pathname])

  return (
    <div className="lg:-mx-2 not-typeset">
      <aside className="hidden self-start pb-12 lg:sticky lg:top-30 lg:flex lg:max-h-[calc(100vh-6rem)] lg:min-h-0 lg:flex-col">
        <ListBox
          ref={listRef}
          aria-label="Docs navigation"
          selectedKeys={selectedHref ? [selectedHref] : []}
          selectionMode="single"
          className="scrollbar-thin scroll-fade-b min-h-0 flex-1 space-y-0.5 overflow-y-auto pr-2 outline-hidden"
        >
          <ListBoxSection className="mb-8">
            <SidebarItem
              textValue="Introduction"
              href="/docs"
              render={(props) => <NextLink href="/docs" {...(props as any)} />}
            >
              <BookOpenIcon />
              Introduction
            </SidebarItem>
            <SidebarItem
              textValue="Components"
              href="/docs/components/api-reference"
              render={(props) => (
                <NextLink href="/docs/components/api-reference" {...(props as any)} />
              )}
            >
              <CubeIcon />
              Components
            </SidebarItem>
            <SidebarItem
              textValue="Components"
              href="/blog"
              render={(props) => <NextLink href="/blog" {...(props as any)} />}
            >
              <NewspaperIcon />
              Blog
            </SidebarItem>
            <SidebarItem textValue="Repositories" href="#">
              <GithubIcon />
              Repositories
            </SidebarItem>
            <SidebarItem textValue="Community" href="#">
              <DiscordIcon />
              Community
            </SidebarItem>
          </ListBoxSection>
          {sections.map((section, index) => (
            <ListBoxSection
              key={section.id}
              id={section.id}
              aria-label={section.textValue}
              className="space-y-0.5"
            >
              <Header className={twJoin("mb-3 px-3 text-sm/4", index === 0 ? "pt-0" : "pt-6")}>
                {section.label}
              </Header>
              {section.items.map((item) => {
                const active = normalizePath(item.href) === pathname

                return (
                  <SidebarItem
                    id={item.href}
                    key={item.href}
                    textValue={`${section.textValue} ${item.textValue}`}
                    data-docs-current={active ? "true" : undefined}
                    href={item.href}
                    render={(props) => <NextLink href={item.href} {...(props as any)} />}
                  >
                    {item.label}
                    {item.status ? <SidebarStatus status={item.status} /> : null}
                  </SidebarItem>
                )
              })}
            </ListBoxSection>
          ))}
        </ListBox>
      </aside>
    </div>
  )
}

const statusIntents: Record<string, NonNullable<BadgeProps["intent"]>> = {
  new: "success",
  update: "info",
  updated: "info",
  beta: "warning",
  deprecated: "danger",
}

function SidebarStatus({ status }: { status: string }) {
  return (
    <Badge
      className="absolute right-2 shrink-0 capitalize"
      intent={statusIntents[status.toLowerCase()] ?? "secondary"}
    >
      {status}
    </Badge>
  )
}

function SidebarItem(props: ListBoxItemProps) {
  return (
    <ListBoxItem
      {...props}
      className={twJoin(
        "relative flex cursor-pointer items-center gap-x-3 rounded-md px-3 py-2 text-sm outline-hidden *:[svg]:size-4",
        "selected:bg-accent selected:text-accent-fg text-muted-fg transition-colors hover:bg-muted hover:text-fg",
        "focus-visible:bg-accent focus-visible:text-accent-fg",
      )}
    />
  )
}
