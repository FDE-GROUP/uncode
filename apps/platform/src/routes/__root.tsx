import { Outlet } from '@tanstack/react-router'
import { Navbar } from '../components/Navbar'
import { useEvents } from '../hooks/useEvents'

export function RootLayout() {
  const { connected } = useEvents()

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-root">
      <Navbar />
      <main className="flex min-h-0 flex-1 flex-col">
        <Outlet />
      </main>
      {/* Connection status indicator */}
      <div className="flex items-center justify-end border-t border-border-subtle bg-root px-4 py-1">
        <span className="flex items-center gap-1.5 text-[10px] text-text-muted">
          <span
            className={`inline-block h-1.5 w-1.5 rounded-full ${
              connected ? 'bg-green-400' : 'bg-red-400'
            }`}
          />
          {connected ? 'Live' : 'Disconnected'}
        </span>
      </div>
    </div>
  )
}
