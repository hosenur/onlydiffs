function literal(value) {
  return {
    type: "Literal",
    value,
    raw: JSON.stringify(value),
  }
}

function tabsToEstree(tabs) {
  return {
    type: "Program",
    sourceType: "module",
    body: [
      {
        type: "ExpressionStatement",
        expression: {
          type: "ArrayExpression",
          elements: tabs.map((tab) => ({
            type: "ObjectExpression",
            properties: Object.entries(tab).map(([key, value]) => ({
              type: "Property",
              method: false,
              shorthand: false,
              computed: false,
              kind: "init",
              key: literal(key),
              value: literal(value),
            })),
          })),
        },
      },
    ],
  }
}

export default function remarkCodeGroup() {
  return (tree) => {
    function walk(node) {
      if (!node || !Array.isArray(node.children)) return

      for (const child of node.children) {
        if (child.type === "mdxJsxFlowElement" && child.name === "CodeGroup") {
          const tabs = child.children
            .filter((item) => item.type === "code")
            .map((item) => ({
              title: item.meta?.trim() || item.lang || "txt",
              lang: item.lang || "txt",
              code: item.value,
            }))

          if (tabs.length > 0) {
            child.attributes = [
              ...(child.attributes ?? []),
              {
                type: "mdxJsxAttribute",
                name: "tabs",
                value: {
                  type: "mdxJsxAttributeValueExpression",
                  value: JSON.stringify(tabs),
                  data: {
                    estree: tabsToEstree(tabs),
                  },
                },
              },
            ]
            child.children = []
          }
        }

        walk(child)
      }
    }

    walk(tree)
  }
}
