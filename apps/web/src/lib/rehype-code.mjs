import {
  transformerNotationDiff,
  transformerNotationFocus,
  transformerNotationHighlight,
  transformerNotationWordHighlight,
} from "@shikijs/transformers"
import rehypePrettyCode from "rehype-pretty-code"
import { codeToHast } from "shiki"

const themes = {
  light: "light-plus",
  dark: "nord",
}

const transformers = [
  transformerNotationDiff(),
  transformerNotationHighlight(),
  transformerNotationWordHighlight(),
  transformerNotationFocus(),
]

function getCodeMetadata(tree) {
  const metadata = []

  function walk(node, parent) {
    if (node?.type === "element" && node.tagName === "code" && parent?.tagName === "pre") {
      const meta = typeof node.data?.meta === "string" ? node.data.meta : ""
      const titleMatch = meta.match(/title=(?:"([^"]+)"|'([^']+)'|([^\s]+))/)
      const lineNumbersMatch = meta.match(/(?:^|\s)lineNumbers(?:=(\d+))?(?:\s|$)/)

      metadata.push({
        title: titleMatch?.[1] ?? titleMatch?.[2] ?? titleMatch?.[3],
        allowCopy: !/(?:^|\s)noCopy(?:\s|$)/.test(meta),
        lineNumbers: Boolean(lineNumbersMatch),
        lineNumberStart: lineNumbersMatch?.[1] ? Number(lineNumbersMatch[1]) : undefined,
      })
    }

    for (const child of node?.children ?? []) {
      walk(child, node)
    }
  }

  walk(tree)
  return metadata
}

function normalizeCodeFigures(tree, metadata) {
  let blockIndex = 0

  function walk(node) {
    if (!Array.isArray(node?.children)) return

    node.children = node.children.flatMap((child) => {
      if (
        child.type === "element" &&
        child.tagName === "figure" &&
        Object.hasOwn(child.properties ?? {}, "data-rehype-pretty-code-figure")
      ) {
        const pre = child.children.find(
          (item) => item.type === "element" && item.tagName === "pre",
        )
        const block = metadata[blockIndex++]

        if (!pre || !block) return child

        pre.properties = {
          ...pre.properties,
          title: block.title,
          allowCopy: block.allowCopy ? undefined : "false",
          "data-line-numbers": block.lineNumbers || undefined,
          "data-line-numbers-start": block.lineNumberStart,
        }

        return pre
      }

      walk(child)
      return child
    })
  }

  walk(tree)
}

function getText(node) {
  if (node?.type === "text") return node.value
  return (node?.children ?? []).map(getText).join("")
}

function getHighlightedLines(tree) {
  const lines = []

  function walk(node) {
    const classNames = Array.isArray(node?.properties?.className)
      ? node.properties.className
      : typeof node?.properties?.class === "string"
        ? node.properties.class.split(/\s+/)
        : []

    if (
      node?.type === "element" &&
      (classNames.includes("line") || Object.hasOwn(node.properties ?? {}, "data-line"))
    ) {
      lines.push(node)
      return
    }

    for (const child of node?.children ?? []) walk(child)
  }

  walk(tree)
  return lines
}

async function highlightNestedMdxFences(tree) {
  const codeBlocks = []

  function collect(node) {
    if (
      node?.type === "element" &&
      node.tagName === "code" &&
      node.properties?.["data-language"] === "mdx"
    ) {
      codeBlocks.push(node)
    }

    for (const child of node?.children ?? []) collect(child)
  }

  collect(tree)

  await Promise.all(
    codeBlocks.map(async (code) => {
      const lines = code.children.filter(
        (node) =>
          node.type === "element" && Object.hasOwn(node.properties ?? {}, "data-line"),
      )
      let fenceStart = -1
      let language = "txt"

      for (let index = 0; index < lines.length; index++) {
        const match = getText(lines[index]).match(/^\s*```([^\s`]*)/)
        if (!match) continue

        if (fenceStart === -1) {
          fenceStart = index
          language = match[1] || "txt"
          continue
        }

        const sourceLines = lines.slice(fenceStart + 1, index)
        const highlighted = await codeToHast(sourceLines.map(getText).join("\n"), {
          lang: language,
          themes: themes,
          defaultColor: false,
        })
        const highlightedLines = getHighlightedLines(highlighted)

        sourceLines.forEach((line, lineIndex) => {
          if (highlightedLines[lineIndex]) {
            line.children = highlightedLines[lineIndex].children
          }
        })

        fenceStart = -1
        language = "txt"
      }
    }),
  )
}

export default function rehypeCode() {
  const prettyCode = rehypePrettyCode({
    theme: themes,
    defaultLang: "ts",
    keepBackground: false,
    transformers,
  })

  return async (tree, file) => {
    const metadata = getCodeMetadata(tree)
    await prettyCode?.(tree, file)
    await highlightNestedMdxFences(tree)
    normalizeCodeFigures(tree, metadata)
  }
}
