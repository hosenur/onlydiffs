import {
  transformerNotationDiff,
  transformerNotationFocus,
  transformerNotationHighlight,
  transformerNotationWordHighlight,
} from "@shikijs/transformers"
import { Children, isValidElement, type ReactElement, type ReactNode } from "react"
import { codeToHtml } from "shiki"
import { CodeGroupClient, CodeGroupPanel } from "@/components/mdx/code-group-client"
import { themes } from "@/lib/docs"

interface CodeTab {
  title: string
  lang: string
  code: string
}

const transformers = [
  transformerNotationDiff(),
  transformerNotationHighlight(),
  transformerNotationWordHighlight(),
  transformerNotationFocus(),
]

interface CodeGroupProps {
  asMenu?: boolean
  children?: React.ReactNode
  tabs?: CodeTab[]
}

interface ElementProps {
  children?: React.ReactNode
  className?: string
  "data-language"?: string
  lang?: string
  metastring?: string
  title?: string
}

function getTextContent(value: ReactNode): string {
  if (typeof value === "string" || typeof value === "number") {
    return String(value)
  }

  if (Array.isArray(value)) {
    return value.map(getTextContent).join("")
  }

  if (isValidElement<ElementProps>(value)) {
    return getTextContent(value.props.children)
  }

  return ""
}

function getLanguage(props: ElementProps) {
  const languageClass = props.className?.match(/(?:^|\s)language-([^\s]+)/)?.[1]
  return props.lang ?? props["data-language"] ?? languageClass ?? "txt"
}

function getTitle(props: ElementProps, fallback: string) {
  return props.title ?? props.metastring?.trim() ?? fallback
}

function collectCodeTabs(children: React.ReactNode, tabs: CodeTab[] = []): CodeTab[] {
  Children.forEach(children, (child) => {
    if (!isValidElement<ElementProps>(child)) {
      return
    }

    const element = child as ReactElement<ElementProps>

    if (element.type === "code") {
      const lang = getLanguage(element.props)

      tabs.push({
        title: getTitle(element.props, lang),
        lang,
        code: getTextContent(element.props.children).trim(),
      })
      return
    }

    collectCodeTabs(element.props.children, tabs)
  })

  return tabs
}

function parseFencedCodeTabs(value: string): CodeTab[] {
  const tabs: CodeTab[] = []
  const fencePattern = /```([^\s`]*)?([^\n`]*)\n([\s\S]*?)\n```/g

  for (const match of value.matchAll(fencePattern)) {
    const lang = match[1]?.trim() || "txt"
    const title = match[2]?.trim() || lang
    const code = match[3]?.trim() ?? ""

    tabs.push({
      title,
      lang,
      code,
    })
  }

  return tabs
}

export async function CodeGroup({ asMenu = false, children, tabs: tabsProp }: CodeGroupProps) {
  const tabs = tabsProp ?? collectCodeTabs(children)
  const normalizedTabs = tabs.length > 0 ? tabs : parseFencedCodeTabs(getTextContent(children))
  const firstTab = normalizedTabs[0]

  if (!firstTab) {
    return null
  }

  const highlightedTabs = await Promise.all(
    normalizedTabs.map(async (tab) => {
      const code = tab.code.trim()

      return {
        ...tab,
        code,
        highlightedCode: await codeToHtml(code, {
          lang: tab.lang,
          themes: themes,
          defaultColor: false,
          transformers,
        }),
      }
    }),
  )

  return (
    <CodeGroupClient asMenu={asMenu} tabs={highlightedTabs}>
      {highlightedTabs.map((tab) => (
        <CodeGroupPanel code={tab.code} id={tab.title} key={tab.title}>
          <div dangerouslySetInnerHTML={{ __html: tab.highlightedCode }} />
        </CodeGroupPanel>
      ))}
    </CodeGroupClient>
  )
}

export const CodeGroups = CodeGroup
