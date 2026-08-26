import type { MetadataRoute } from "next"
import { app } from "@/config/app"

export default function robots(): MetadataRoute.Robots {
  return {
    rules: {
      userAgent: "*",
      allow: "/",
      disallow: ["/llm", "/llm/"],
    },
    sitemap: `${app.url}/sitemap.xml`,
  }
}
