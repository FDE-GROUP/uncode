// @ts-nocheck
import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts'
import { ThemeProvider } from './themes/ThemeContext'
import type { SessionSummary, SessionDetail } from './types'

const API = 'http://127.0.0.1:3000'
type Tab = 'sessions' | 'issues' | 'dashboard'

function App() {
  const [tab, setTab] = useState<Tab>('sessions')
  const [activeSession, setActiveSession] = useState<string | null>(null)

  const { data: sessions, isLoading } = useQuery<SessionSummary[]>({
    queryKey: ['sessions'],
    queryFn: () => fetch(`${API}/api/sessions`).then(r => r.json()),
  })

  const { data: sessionDetail } = useQuery<SessionDetail>({
    queryKey: ['session', activeSession],
    queryFn: () => fetch(`${API}/api/sessions/${activeSession}`).then(r => r.json()),
    enabled: !!activeSession,
  })

  const { data: issues } = useQuery({
    queryKey: ['issues'],
    queryFn: () => fetch('https://api.github.com/repos/FDE-GROUP/uncode/issues?state=open&per_page=20')
      .then(r => r.json()),
    enabled: tab === 'issues',
  })

  const chartData = sessions?.slice(0, 10).reverse().map((s, i) => ({
    name: `#${i + 1}`,
    messages: s.message_count,
  }))

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
            {tabs.map(t => (
              <button key={t.key} onClick={() => setTab(t.key)}
                className={`w-full text-left px-3 py-2 rounded mb-1 ${
                  tab === t.key ? 'bg-elevated text-accent' : 'text-text-secondary hover:bg-hover'}`}>
                {t.icon} {t.label}
              </button>
            ))}
          </nav>
          {tab === 'sessions' && (
            <>
              <h3 className="text-text-secondary text-sm uppercase mb-2">Sessions</h3>
              {isLoading && <p className="text-text-muted">Loading...</p>}
              {sessions?.map(s => (
                <div key={s.id} onClick={() => setActiveSession(s.id)}
                  className={`p-2 my-1 rounded cursor-pointer ${
                    activeSession === s.id ? 'bg-elevated' : 'hover:bg-hover'}`}>
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
              <div className="grid grid-cols-3 gap-4 mb-8">
                <MetricCard title="Total Sessions" value={sessions?.length || 0} />
                <MetricCard title="Open Issues" value={issues?.length || 0} />
                <MetricCard title="Avg Messages" value={
                  sessions?.length ? Math.round(sessions.reduce((a, s) => a + s.message_count, 0) / sessions.length) : 0
                } />
              </div>
              {chartData && (
                <div className="bg-surface rounded-lg p-6 border border-elevated">
                  <h3 className="text-lg font-semibold mb-4">Messages per Session</h3>
                  <ResponsiveContainer width="100%" height={300}>
                    <BarChart data={chartData}>
                      <XAxis dataKey="name" stroke="#888" />
                      <YAxis stroke="#888" />
                      <Tooltip
                        contentStyle={{ background: '#1a1a2e', border: '1px solid #333', borderRadius: 8 }}
                        labelStyle={{ color: '#e0e0e0' }}
                      />
                      <Bar dataKey="messages" fill="#7c3aed" radius={[4, 4, 0, 0]} />
                    </BarChart>
                  </ResponsiveContainer>
                </div>
              )}
            </div>
          )}

          {tab === 'issues' && (
            <div>
              <h2 className="text-2xl font-bold mb-6">🐛 GitHub Issues</h2>
              <div className="space-y-2">
                {issues?.map((issue) => (
                  <a key={issue.id} href={issue.html_url} target="_blank"
                    className="block bg-surface rounded-lg p-4 border border-elevated hover:border-accent transition-colors">
                    <div className="flex items-center gap-2 mb-1">
                      <span className={`w-2 h-2 rounded-full ${issue.state === 'open' ? 'bg-green-400' : 'bg-purple-400'}`} />
                      <span className="font-medium text-accent">#{issue.number}</span>
                      <span className="flex-1">{issue.title}</span>
                    </div>
                    <div className="text-text-muted text-xs ml-4">
                      {issue.labels?.map((l) => (
                        <span key={l.name} className="inline-block px-2 py-0.5 mr-1 rounded border border-elevated"
                          style={{ color: `#${l.color}`, borderColor: `#${l.color}40` }}>
                          {l.name}
                        </span>
                      ))}
                    </div>
                  </a>
                ))}
              </div>
            </div>
          )}

          {tab === 'sessions' && activeSession && sessionDetail && (
            <div>
              <h2 className="text-2xl font-bold mb-2">
                {sessionDetail.title || `Session ${sessionDetail.id.slice(0, 8)}`}
              </h2>
              <p className="text-text-secondary text-sm mb-6">
                {sessionDetail.model} &nbsp;|&nbsp; {sessionDetail.working_dir}
              </p>
              <div className="space-y-1">
                {sessionDetail.entries?.map((entry, i) => (
                  <div key={i} className={`p-2 rounded border-l-4 ${
                    entry.type === 'system' ? 'bg-surface border-accent' : 'bg-elevated border-hover'}`}>
                    {entry.type === 'message' && (
                      <>
                        <span className="text-accent text-xs">{entry.role}</span>
                        {entry.content?.map((block, j) => (
                          <div key={j} className="mt-1">
                            {block.type === 'text' && <span>{block.text?.slice(0, 200)}</span>}
                            {block.type === 'tool_call' && <span className="text-accent-bright">🔧 {block.name}</span>}
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
                      <span className="text-xs text-text-secondary">📌 {entry.event}</span>
                    )}
                  </div>
                ))}
              </div>
            </div>
          )}
        </main>
      </div>
    </ThemeProvider>
  )
}

function MetricCard({ title, value }: { title: string; value: number }) {
  return (
    <div className="bg-surface rounded-lg p-5 text-center border border-elevated">
      <div className="text-text-muted text-sm mb-2">{title}</div>
      <div className="text-3xl font-bold text-accent">{value}</div>
    </div>
  )
}

export default App
