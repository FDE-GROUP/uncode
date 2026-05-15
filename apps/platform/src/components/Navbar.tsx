import { Link, useNavigate, useRouterState } from '@tanstack/react-router'
import { useEffect } from 'react'
import { BarChart3, Bug, Home, Settings, Terminal } from '@/lib/lucide-icons'
import { ThemeToggle } from './ThemeToggle'

interface NavItem {
  to: string
  label: string
  icon: React.ComponentType<{ className?: string }>
  shortcut: string
}

const NAV_ITEMS: NavItem[] = [
  { to: '/', label: 'Home', icon: Home, shortcut: 'H' },
  { to: '/sessions', label: 'Sessions', icon: Terminal, shortcut: 'S' },
  { to: '/issues', label: 'Issues', icon: Bug, shortcut: 'I' },
  { to: '/dashboard', label: 'Dashboard', icon: BarChart3, shortcut: 'D' },
]

export function Navbar() {
  const navigate = useNavigate()
  const routerState = useRouterState()
  const currentPath = routerState.location.pathname

  const isActive = (item: NavItem) => {
    if (item.to === '/') return currentPath === '/'
    return currentPath.startsWith(item.to)
  }

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!e.metaKey && !e.ctrlKey) return
      e.preventDefault()

      const keyMap: Record<string, string> = {
        h: '/',
        s: '/sessions',
        i: '/issues',
        d: '/dashboard',
        ',': '/settings',
      }
      const target = keyMap[e.key]
      if (target) {
        navigate({ to: target })
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [navigate])

  const settingsActive = currentPath === '/settings'

  return (
    <header className="flex items-center justify-between border-b border-dashed border-border-subtle bg-root px-5 py-3">
      <div className="flex items-center gap-5">
        <div className="flex items-center gap-2.5">
          <span className="text-[15px] font-semibold tracking-tight text-accent">
            uncode
          </span>
        </div>

        <nav className="flex items-center gap-1">
          {NAV_ITEMS.map((item) => {
            const active = isActive(item)
            const Icon = item.icon
            return (
              <Link
                key={item.to}
                to={item.to}
                className={`flex cursor-pointer items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-colors ${
                  active
                    ? 'bg-accent/15 text-accent'
                    : 'text-text-secondary hover:bg-hover hover:text-text-primary'
                }`}
                title={`${item.label} (Ctrl+${item.shortcut})`}
              >
                <Icon className="h-4 w-4" />
                <span>{item.label}</span>
              </Link>
            )
          })}
        </nav>
      </div>

      <div className="flex items-center gap-1">
        <Link
          to="/settings"
          className={`flex cursor-pointer items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-colors ${
            settingsActive
              ? 'bg-accent/15 text-accent'
              : 'text-text-secondary hover:bg-hover hover:text-text-primary'
          }`}
        >
          <Settings className="h-4 w-4" />
          <span>Settings</span>
        </Link>
        <ThemeToggle />
      </div>
    </header>
  )
}
