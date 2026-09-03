"use client"

import { twMerge } from "tailwind-merge"
import { Logo } from "@/components/logo"
import { ThemeSwitcherFooter } from "@/components/theme-switcher-footer"
import { Link as PrimitiveLink } from "@/components/ui/link"
import { Text } from "@/components/ui/text"
import { app } from "@/config/app"
import { cx } from "@/lib/primitive"

const sections = [
  {
    title: "Product",
    links: [
      { name: "Install", href: "/docs/getting-started/installation" },
      { name: "First review", href: "/docs/getting-started/first-review" },
      { name: "Claude feedback", href: "/docs/workflows/claude-feedback" },
      { name: "SSH repositories", href: "/docs/remote-repositories/connect-an-ssh-host" },
    ],
  },
  {
    title: "Reference",
    links: [
      { name: "Keyboard shortcuts", href: "/docs/reference/keyboard-shortcuts" },
      { name: "Settings and data", href: "/docs/reference/settings-and-local-data" },
      { name: "Limits", href: "/docs/reference/limits-and-platforms" },
      { name: "Updates", href: "/docs/reference/updates" },
    ],
  },
  {
    title: "Project",
    links: [
      { name: "GitHub", href: "https://github.com/hosenur/onlydiffs" },
      { name: "Releases", href: "https://github.com/hosenur/onlydiffs/releases" },
      { name: "Report an issue", href: "https://github.com/hosenur/onlydiffs/issues/new" },
    ],
  },
] as const

export function Footer({ className }: { className?: string }) {
  return (
    <footer className={twMerge("mt-6 border-t bg-bg py-10 lg:ml-28", className)}>
      <div className="grid gap-10 sm:grid-cols-[1fr_2fr]">
        <div>
          <Logo className="size-6" />
          <Text className="mt-3 max-w-xs text-balance">{app.description}</Text>
        </div>
        <div className="grid gap-8 sm:grid-cols-3">
          {sections.map((section) => (
            <div key={section.title}>
              <h3 className="font-semibold text-fg text-sm/6">{section.title}</h3>
              <ul className="mt-4 space-y-2.5">
                {section.links.map((item) => (
                  <li key={item.name}>
                    <Link className="font-normal" href={item.href}>
                      {item.name}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </div>
      <div className="mt-10 flex flex-col justify-between gap-5 border-t pt-6 sm:flex-row sm:items-center">
        <p className="text-muted-fg text-sm/6">
          &copy; {new Date().getFullYear()} {app.name}
        </p>
        <ThemeSwitcherFooter />
      </div>
    </footer>
  )
}

export function Link({ className, ...props }: React.ComponentProps<typeof PrimitiveLink>) {
  return (
    <PrimitiveLink
      className={cx("text-base/6 text-fg/80 hover:text-fg sm:text-sm/6", className)}
      {...props}
    />
  )
}
