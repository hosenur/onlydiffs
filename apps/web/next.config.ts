import { fileURLToPath } from "node:url"
import createMDX from "@next/mdx"
import type { NextConfig } from "next"

// Resolve local MDX plugins to absolute paths so Turbopack can load them from its worker.
const remarkCodeGroup = fileURLToPath(new URL("./src/lib/remark-code-group.mjs", import.meta.url))
const rehypeCode = fileURLToPath(new URL("./src/lib/rehype-code.mjs", import.meta.url))

const nextConfig: NextConfig = {
  // @onlydiffs/ui ships TypeScript source rather than a build, so Next has to
  // compile it the way it compiles this app.
  transpilePackages: ["@onlydiffs/ui"],
  pageExtensions: ["js", "jsx", "md", "mdx", "ts", "tsx"],
  reactCompiler: true,
  async rewrites() {
    return [
      {
        source: "/docs.md",
        destination: "/llm",
      },
      {
        source: "/docs/:path*.md",
        destination: "/llm/:path*",
      },
    ]
  },
}

const withMDX = createMDX({
  options: {
    remarkPlugins: ["remark-frontmatter", "remark-gfm", remarkCodeGroup],
    rehypePlugins: [rehypeCode, "rehype-slug"],
  },
})

export default withMDX(nextConfig)
