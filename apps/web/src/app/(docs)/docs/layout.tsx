import { DocsSidebar } from "@/components/docs/sidebar"
import { Footer } from "@/components/footer"
import { Navigation } from "@/components/navigation"
import { Container } from "@onlydiffs/ui/container"
import { source } from "@/lib/source"

export default function Layout({ children }: { children: React.ReactNode }) {
  const tree = source.getPageTree()

  return (
    <div className="min-h-screen">
      <Navigation docsTree={tree} />
      <Container>
        <div className="grid grid-cols-1 gap-8 lg:grid-cols-[240px_minmax(0,1fr)] lg:py-16">
          <DocsSidebar tree={tree} />
          <main className="mx-auto w-full min-w-0">
            {children}
            <Footer />
          </main>
        </div>
      </Container>
    </div>
  )
}
