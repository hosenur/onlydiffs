"use client"

import Link from "next/link"
import { Text } from "@/components/ui/text"

const components = [
  {
    label: "API reference",
    description: "Document APIs with a consistent reference layout.",
    href: "/docs/components/api-reference",
  },
  {
    label: "Badge",
    description: "Show metadata in compact labels.",
    href: "/docs/components/badge",
  },
  {
    label: "Callouts",
    description: "Highlight guidance without interrupting the flow.",
    href: "/docs/components/callouts",
  },
  {
    label: "Changelog",
    description: "Publish product updates in a readable timeline.",
    href: "/docs/components/changelog",
  },
  {
    label: "Code blocks",
    description: "Show code with highlighting and controls.",
    href: "/docs/components/code-blocks",
  },
  {
    label: "Code groups",
    description: "Switch between related code examples.",
    href: "/docs/components/code-groups",
  },
]
export function Components() {
  return (
    <div className="-mx-4 mt-6">
      <ul className="not-typeset mb-6 grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3">
        {components.map((c) => (
          <li key={c.href}>
            <Link className="block p-4 hover:bg-secondary lg:rounded-lg" href={c.href}>
              <h3 className="font-semibold">{c.label}</h3>
              <Text className="mt-1">{c.description}</Text>
            </Link>
          </li>
        ))}
      </ul>
    </div>
  )
}
