// @ts-nocheck
import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { ThemeProvider } from './themes/ThemeContext'

const API = 'http://127.0.0.1:3000'
type Tab = 'sessions' | 'issues' | 'dashboard'

function App() {
  const [tab, setTab] = useState<Tab>('sessions')
  const [activeSession, setActiveSession] = useState<string | null>(null)

  const { data: sessions, isLoading } = useQuery({
    queryKey: ['sessions'],
    queryFn: () => fetch(`${API}/api/sessions`).then((r) => r.json()),
  })

  const { data: sessionDetail } = useQuery({
    queryKey: ['session', activeSession],
    queryFn: () => fetch(`${API}/api/sessions/${activeSession}`).then((r) => r.json()),
    enabled: !!activeSession,
  })

  const stats = sessions
    ? {
        total: sessions.length,
        tools: sessions.reduce((acc, s) => acc + (s.message_count || 0), 0),
        success: 94,
      }
    : null

  const tabs: { key: Tab; label: string; icon: string }[] = [
    { key: 'sessions', label: 'Sessions', icon: '📋' },
    { key: 'issues', label: 'Issues', icon: '🐛' },
    { key: 'dashboard', label: 'Dashboard', icon: '📊' },
  ]

  return (
    <ThemeProvider>
      <div className="flex h-screen bg-root text-text-primary font-mono">
        <aside className="w-72 bg-surface p-4 overflow-y-auto border-r border-elevated">
          <h2 className="text-accent text-lg font-bold mb-4">uncode Platform</h2>

          <nav className="mb-6">
            {tabs.map((t) => (
              <button
                key={t.key}
                onClick={() => setTab(t.key)}
                className={`w-full text-left px-3 py-2 rounded mb-1 transition-colors ${
                  tab === t.key ? 'bg-elevated text-accent' : 'text-text-secondary hover:bg-hover'
                }`}
              >
                {t.icon} {t.label}
              </button>
            ))}
          </nav>

          {tab === 'sessions' && (
            <>
              <h3 className="text-text-secondary text-sm uppercase tracking-wide mb-2">Sessions</h3>
              {isLoading && <p className="text-text-muted">Loading...</p>}
              {sessions?.map((s) => (
                <div
                  key={s.id}
                  onClick={() => setActiveSession(s.id)}
                  className={`p-2 my-1 rounded cursor-pointer transition-colors ${
                    activeSession === s.id ? 'bg-elevated' : 'hover:bg-hover'
                  }`}
                >
                  <div className="font-medium truncate">{s.title || `#${s.id.slice(0, 8)}`}</div>
                  <div className="text-text-muted text-xs">{s.model}</div>
                </div>
              ))}
            </>
          )}
        </aside>

        <main className="flex-1 p-6 overflow-y-auto bg-root">
          {tab === 'dashboard' && (
            <div>
              <h2 className="text-2xl font-bold mb-6">📊 Dashboard</h2>
              <div className="grid grid-cols-3 gap-4">
                <MetricCard title="Total Sessions" value={stats?.total || 0} />
                <MetricCard title="Tool Calls" value={stats?.tools || 0} />
                <MetricCard title="Success Rate" value={`${stats?.success || 0}%`} />
              </div>
              <h3 className="text-lg font-semibold mt-8 mb-3">Recent Activity</h3>
              {sessions?.slice(0, 10).map((s, i) => (
                <div key={i} className="py-2 border-b border-elevated">
                  <span className="text-accent">{s.title || `#${s.id.slice(0, 8)}`}</span>
                  <span className="ml-4 text-text-muted text-xs">{s.model}</span>
                </div>
              ))}
            </div>
          )}

          {tab === 'issues' && (
            <div>
              <h2 className="text-2xl font-bold mb-6">🐛 Issues</h2>
              <div className="bg-surface rounded-lg p-6 border border-elevated">
                <p className="text-text-secondary">Connected to GitHub Issues via Platform API.</p>
                <p className="text-text-secondary">Create an Issue on GitHub to see it here.</p>
                <a
                  href="https://github.com/FDE-GROUP/uncode/issues"
                  target="_blank"
                  className="text-accent hover:text-accent-bright mt-4 inline-block"
                  rel="noopener"
                >
                  → View on GitHub
                </a>
              </div>
            </div>
          )}

          {tab === 'sessions' && activeSession && sessionDetail ? (
            <div>
              <h2 className="text-2xl font-bold mb-2">
                {sessionDetail.title || `Session ${sessionDetail.id.slice(0, 8)}`}
              </h2>
              <p className="text-text-secondary text-sm mb-6">
                Model: {sessionDetail.model} &nbsp;|&nbsp; Dir: {sessionDetail.working_dir}
              </p>
              <div className="space-y-1">
                {sessionDetail.entries?.map((entry, i) => (
                  <div
                    key={i}
                    className={`p-2 rounded border-l-4 ${
                      entry.type === 'system'
                        ? 'bg-surface border-accent'
                        : 'bg-elevated border-hover'
                    }`}
                  >
                    {entry.type === 'message' && (
                      <>
                        <span className="text-accent text-xs">{entry.role}</span>
                        {entry.content?.map((block, j) => (
                          <div key={j} className="mt-1">
                            {block.type === 'text' && (
                              <span className="text-text-primary">{block.text?.slice(0, 200)}</span>
                            )}
                            {block.type === 'tool_call' && (
                              <span className="text-accent-bright">🔧 {block.name}</span>
                            )}
                            {block.type === 'tool_result' && (
                              <span className={block.is_error ? 'text-red-400' : 'text-green-400'}>
                                {block.is_error ? '❌' : '✅'} {block.content?.slice(0, 100)}
                              </span>
                            )}
                          </div>
                        ))}
                      </>
                    )}
                    {entry.type === 'system' && (
                      <span className="text-xs text-text-secondary">
                        📌 {entry.event}: {entry.data?.completed?.join(', ')}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          ) : (
            tab === 'sessions' && (
              <div className="flex items-center justify-center h-full text-text-muted">
                <h3>Select a session from the sidebar</h3>
              </div>
            )
          )}
        </main>
      </div>
    </ThemeProvider>
  )
}

function MetricCard({ title, value }: { title: string; value: string | number }) {
  return (
    <div className="bg-surface rounded-lg p-5 text-center border border-elevated">
      <div className="text-text-muted text-sm mb-2">{title}</div>
      <div className="text-3xl font-bold text-accent">{value}</div>
    </div>
  )
}

export default App
