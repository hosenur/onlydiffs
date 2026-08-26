import { Footer } from "@/components/footer"
import { Navigation } from "@/components/navigation"

export default function Layout({ children }: { children: React.ReactNode }) {
  return (
    <div>
      <Navigation />
      {children}
      <Footer className="pl-4 sm:pl-6 lg:pl-8" />
    </div>
  )
}
