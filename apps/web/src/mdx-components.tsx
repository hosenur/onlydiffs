import { Badge } from "@onlydiffs/ui/badge"
import {
  Disclosure,
  DisclosureGroup,
  DisclosurePanel,
  DisclosureTrigger,
} from "@onlydiffs/ui/disclosure-group"
import { Tab, TabList, TabPanel, Tabs } from "@onlydiffs/ui/tabs"
import type { MDXComponents } from "mdx/types"
import Link from "next/link"
import type { ComponentPropsWithoutRef } from "react"
import { twMerge } from "tailwind-merge"
import {
  ApiEndpoint,
  ApiParameter,
  ApiParameters,
  ApiResponse,
} from "@/components/mdx/api-reference"
import { Callout } from "@/components/mdx/callout"
import { Changelog, ChangelogEntry } from "@/components/mdx/changelog"
import { CodeGroup, CodeGroups } from "@/components/mdx/code-group"
import {
  CodePen,
  CodeSandbox,
  Figma,
  Replit,
  SoundCloud,
  Spotify,
  Tweet,
  Vimeo,
  YouTube,
} from "@/components/mdx/embed"
import { FileTree, FileTreeItem } from "@/components/mdx/file-tree"
import { Heading } from "@/components/mdx/heading"
import { DocsImage } from "@/components/mdx/image"
import { PackageCommand } from "@/components/mdx/package-command"
import { PlainCode, Pre } from "@/components/mdx/plain-code"
import {
  SnippetTab,
  SnippetTabPanel,
  SnippetTabPanels,
  SnippetTabsList,
} from "@/components/ui/snippet"
import { textLinkStyles } from "@/components/ui/text"

const linkStyles = textLinkStyles({ className: "no-typeset no-underline" })
function MdxLink(props: ComponentPropsWithoutRef<"a">) {
  const href = props.href ?? ""

  if (href.startsWith("/")) {
    return <Link {...props} href={href} className={linkStyles} />
  }

  return (
    <a
      {...props}
      className={linkStyles}
      rel={href.startsWith("http") ? "noreferrer" : props.rel}
      target={href.startsWith("http") ? "_blank" : props.target}
    />
  )
}

export function getMDXComponents(components?: MDXComponents): MDXComponents {
  return {
    h2: (props) => <Heading {...props} as="h2" className={twMerge("text-xl", props.className)} />,
    h3: (props) => <Heading {...props} as="h3" className={twMerge("text-lg", props.className)} />,
    img: DocsImage,
    h4: (props) => <Heading {...props} as="h4" className={twMerge("text-base", props.className)} />,
    a: MdxLink,
    ul: (props) => <ul {...props} className="my-5 list-disc space-y-2 pl-6" />,
    ol: (props) => <ol {...props} className="my-5 list-decimal space-y-2 pl-6" />,
    li: (props) => <li {...props} className="pl-1 leading-7" />,
    pre: (props: React.ComponentProps<typeof PlainCode>) => (
      <PlainCode {...props}>
        <Pre>{props.children}</Pre>
      </PlainCode>
    ),
    table: (props) => (
      <div className="-mx-4 not-typeset my-6 overflow-x-auto border-y lg:mx-0 lg:rounded-lg lg:border">
        <table {...props} className="w-full border-collapse text-sm" />
      </div>
    ),
    tr: (props) => <tr {...props} className="[&:last-child>td]:border-b-0" />,
    th: (props) => (
      <th
        {...props}
        className="whitespace-nowrap border-b px-3 py-3 text-start font-medium dark:bg-muted"
      />
    ),
    td: (props) => <td {...props} className="whitespace-nowrap border-b px-3 py-3" />,
    Badge,
    ApiEndpoint,
    ApiParameter,
    ApiParameters,
    ApiResponse,
    FileTree,
    FileTreeItem,
    Disclosure: (props) => <Disclosure {...props} className="not-typeset inset-ring-0" />,
    DisclosureGroup: (props) => (
      <DisclosureGroup
        allowsMultipleExpanded={false}
        {...props}
        className="not-typeset mt-6 gap-y-1 [--disclosure-expanded-bg:var(--color-muted)]"
      />
    ),
    DisclosurePanel: (props) => (
      <DisclosurePanel allowsMultipleExpanded={false} {...props} className="*:pt-0" />
    ),
    DisclosureTrigger,
    Step: (props: React.ComponentProps<"h3">) => <h3 {...props} />,
    Steps: ({ className, ...props }: React.ComponentProps<"div">) => (
      <div
        className={twMerge(
          "steps [&>h3]:step mb-12 [counter-reset:step] md:ml-4 md:border-l md:pl-8",
          className,
        )}
        {...props}
      />
    ),
    Callout,
    Changelog,
    ChangelogEntry,
    SnippetTab,
    SnippetTabPanel,
    SnippetTabPanels,
    SnippetTabsList,
    PackageCommand,
    CodeGroup,
    CodeGroups,
    YouTube,
    Vimeo,
    Tweet,
    CodePen,
    CodeSandbox,
    Figma,
    Spotify,
    SoundCloud,
    Replit,
    Tabs: (props) => <Tabs {...props} className="not-typeset mt-6" />,
    TabList,
    Tab,
    TabPanel,
    ...components,
  }
}

export function useMDXComponents(components?: MDXComponents): MDXComponents {
  return getMDXComponents(components)
}
