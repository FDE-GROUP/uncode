import { Outlet } from '@tanstack/react-router'
import { Navbar } from '../components/Navbar'

export function RootLayout() {
  return (
    <div className="flex h-screen flex-col overflow-hidden bg-root">
      <Navbar />
      <main className="flex min-h-0 flex-1 flex-col">
        <Outlet />
      </main>
    </div>
  )
}
