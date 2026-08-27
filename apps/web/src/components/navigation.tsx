"use client"

import { MagnifyingGlassIcon } from "@heroicons/react/24/solid"
import Link from "next/link"
import { usePathname } from "next/navigation"
import { useState } from "react"
import { twMerge } from "tailwind-merge"
import { CommandPalette } from "@/components/command-palette"
import { GithubIcon } from "@/components/icons/github-icon"
import { Logo } from "@/components/logo"
import { ResponsiveNavigation } from "@/components/responsive-navigation"
import { ThemeSwitcher } from "@/components/theme-switcher"
import { Button } from "@onlydiffs/ui/button"
import { Container } from "@onlydiffs/ui/container"
import { Keyboard } from "@onlydiffs/ui/keyboard"
import { useIsMobile } from "@/hooks/use-mobile"
import type { PageTreeRoot } from "@/types/content"

export function Navigation({ docsTree }: { docsTree?: PageTreeRoot }) {
  const isMobile = useIsMobile()
  const [open, setOpen] = useState(false)
  return (
    <>
      <CommandPalette tree={docsTree} isOpen={open} onOpenChange={setOpen} />
      <header className="sticky top-0 z-20 bg-bg lg:border-b lg:bg-bg/80 lg:backdrop-blur-2xl">
        <Container>
          <div className="flex h-14 items-center justify-between">
            {isMobile ? <ResponsiveNavigation tree={docsTree} /> : null}
            <div className="flex items-center gap-x-3">
              <Link href="/" aria-label="onlydiffs" className="lg:mr-4">
                <Logo className="size-5" />
              </Link>

              <div className="hidden items-center gap-x-4 lg:flex">
                <NavigationLink href="/docs">Docs</NavigationLink>
                <NavigationLink href="/blog">Blog</NavigationLink>
                <NavigationLink href="/docs/components/changelog">Changelog</NavigationLink>
              </div>
            </div>
            <nav className="flex items-center gap-x-2 text-sm">
              {!isMobile && (
                <Button
                  intent="outline"
                  size="sm"
                  isCircle
                  aria-label="Search"
                  onPress={() => setOpen(true)}
                  className="mr-3 font-normal text-muted-fg hover:text-fg dark:bg-muted"
                >
                  <MagnifyingGlassIcon />
                  <Keyboard className="ml-2">⌘k</Keyboard>
                </Button>
              )}
              <ThemeSwitcher />
              <Button isCircle intent="plain" size="sq-sm" aria-label="Github">
                <GithubIcon />
              </Button>
            </nav>
          </div>
        </Container>
      </header>
    </>
  )
}

export function NavigationLink({ className, ...props }: React.ComponentProps<typeof Link>) {
  const currentPath = usePathname()
  const href = props.href.toString()
  const active = currentPath === href || currentPath.startsWith(`${href}/`)

  return (
    <Link
      className={twMerge(
        "block px-1 py-1 text-sm/6 transition-colors hover:text-fg",
        active ? "text-fg" : "text-muted-fg",
        className,
      )}
      {...props}
    />
  )
}
