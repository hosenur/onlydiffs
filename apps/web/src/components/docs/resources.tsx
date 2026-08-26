import { BookOpenIcon, CubeIcon, DocumentTextIcon, FolderIcon } from "@heroicons/react/24/outline"
import Link from "next/link"
import { Text } from "@/components/ui/text"

const resources = [
  {
    label: "Getting started",
    description: "Set up the template and learn which files to edit.",
    href: "/docs/getting-started/installation",
    icon: BookOpenIcon,
  },
  {
    label: "Components",
    description: "Explore the components included with the template.",
    href: "/docs/components/api-reference",
    icon: CubeIcon,
  },
  {
    label: "Writing pages",
    description: "Add content and metadata to your MDX pages.",
    href: "/docs/writing-pages/frontmatter",
    icon: DocumentTextIcon,
  },
  {
    label: "Organizing content",
    description: "Organize documentation with folders and sections.",
    href: "/docs/organizing-content/folders",
    icon: FolderIcon,
  },
]

export function Resources() {
  return (
    <ul className="not-typeset my-6 grid grid-cols-1 gap-4 sm:grid-cols-2">
      {resources.map(({ label, description, href, icon: Icon }) => (
        <li key={href}>
          <Link
            className="group flex flex-col gap-6 border border-primary/10 bg-primary-subtle/20 p-4 transition-colors hover:border-primary/80 hover:bg-primary-subtle lg:rounded-lg"
            href={href}
          >
            <div className="relative size-10">
              <i
                aria-hidden
                className="relative block size-8 shrink-0 rounded-full border-primary/20 bg-primary-subtle transition-colors group-hover:border-primary/30 group-hover:bg-primary/20 dark:group-hover:bg-primary"
              />
              <div className="absolute right-0.5 bottom-0.5 grid size-7 place-content-center rounded-sm bg-primary/20 backdrop-blur-sm">
                <Icon aria-hidden className="size-4.5 shrink-0 text-primary-subtle-fg" />
              </div>
            </div>
            <div>
              <h3 className="font-semibold">{label}</h3>
              <Text className="mt-1">{description}</Text>
            </div>
          </Link>
        </li>
      ))}
    </ul>
  )
}
