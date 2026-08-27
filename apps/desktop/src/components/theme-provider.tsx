import { createContext, use, useEffect, useState } from "react"
import type { AppTheme } from "@shared/contract"
import { setTheme as setNativeTheme } from "@/lib/ipc"

/** Aliased off the IPC contract so the two cannot drift apart. */
type Theme = AppTheme

type ThemeProviderProps = {
  children: React.ReactNode
  defaultTheme?: Theme
  storageKey?: string
}

type ThemeProviderState = {
  theme: Theme
  setTheme: (theme: Theme) => void
}

const initialState: ThemeProviderState = {
  theme: "system",
  setTheme: () => null,
}

const ThemeProviderContext = createContext<ThemeProviderState>(initialState)

function ThemeProvider({
  children,
  defaultTheme = "system",
  storageKey = "iut",
  ...props
}: ThemeProviderProps) {
  const [theme, setTheme] = useState<Theme>(() => {
    if (typeof window !== "undefined") {
      return (localStorage?.getItem(storageKey) as Theme) || defaultTheme
    }
    return defaultTheme
  })

  useEffect(() => {
    if (typeof window === "undefined") return

    const root = window.document.documentElement
    const query = window.matchMedia("(prefers-color-scheme: dark)")

    const apply = () => {
      root.classList.remove("light", "dark")
      root.classList.add(
        theme === "system" ? (query.matches ? "dark" : "light") : theme,
      )
    }

    apply()

    /*
     * The page can only paint inside the window. The frame around it — title
     * bar, traffic lights — is the OS's, and follows the window's own theme,
     * so it has to be told separately or it stays on the system appearance
     * while the app sits on the opposite one.
     *
     * Deliberately not awaited, and a failure is swallowed: the cost is a
     * frame that keeps its old colour, which is not worth failing a render or
     * surfacing to someone who just picked a theme.
     */
    void setNativeTheme(theme).catch(() => {})

    // Desktop app: the window outlives OS appearance changes, so keep following.
    if (theme !== "system") return
    query.addEventListener("change", apply)
    return () => query.removeEventListener("change", apply)
  }, [theme])

  const value = {
    theme,
    setTheme: (theme: Theme) => {
      if (typeof window !== "undefined") {
        localStorage.setItem(storageKey, theme)
      }
      setTheme(theme)
    },
  }

  return (
    <ThemeProviderContext {...props} value={value}>
      {children}
    </ThemeProviderContext>
  )
}

const useTheme = () => {
  const context = use(ThemeProviderContext)
  if (context === undefined) throw new Error("useTheme must be used within a ThemeProvider")
  return context
}

export { ThemeProvider, useTheme, type Theme, type ThemeProviderProps, type ThemeProviderState }
