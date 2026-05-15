import { useQuery } from '@tanstack/react-query'
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts'
import { fetchApi } from '@/lib/api'
import { useEvents } from '@/hooks/useEvents'
import type { MetricsResponse } from '../types'

function MetricCard({ title, value }: { title: string; value: string | number }) {
  return (
    <div className="rounded-lg border border-border-subtle bg-surface/50 p-5 text-center">
      <div className="mb-2 text-sm text-text-muted">{title}</div>
      <div className="text-3xl font-bold text-accent">{value}</div>
    </div>
  )
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

export function DashboardPage() {
  const { data: metrics, isLoading } = useQuery<MetricsResponse>({
    queryKey: ['metrics'],
    queryFn: () => fetchApi('/api/metrics'),
  })

  const { events, clear } = useEvents()

  if (isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-text-muted">Loading...</p>
      </div>
    )
  }

  const chartData = metrics?.recent_sessions
    .slice()
    .reverse()
    .map((s, i) => ({
      name: s.title || `#${s.id.slice(0, 6)}`,
      messages: s.message_count,
    }))

  const totalTokens = (metrics?.total_input_tokens ?? 0) + (metrics?.total_output_tokens ?? 0)
  const successRate = metrics?.tool_success_rate ?? 1

  return (
    <div className="flex flex-1 flex-col overflow-y-auto p-6">
      <h2 className="mb-6 text-2xl font-bold">Dashboard</h2>

      <div className="mb-8 grid grid-cols-2 gap-4 lg:grid-cols-4">
        <MetricCard title="Total Sessions" value={metrics?.total_sessions ?? 0} />
        <MetricCard title="Tool Calls" value={metrics?.total_tool_calls ?? 0} />
        <MetricCard title="Total Tokens" value={formatTokens(totalTokens)} />
        <MetricCard
          title="Avg Messages"
          value={(metrics?.avg_messages_per_session ?? 0).toFixed(1)}
        />
      </div>

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        {chartData && chartData.length > 0 && (
          <div className="rounded-lg border border-border-subtle bg-surface/50 p-6">
            <h3 className="mb-4 text-lg font-semibold">Recent Sessions</h3>
            <ResponsiveContainer width="100%" height={260}>
              <BarChart data={chartData}>
                <XAxis dataKey="name" stroke="#888" tick={{ fontSize: 11 }} />
                <YAxis stroke="#888" />
                <Tooltip
                  contentStyle={{
                    background: '#1a1a2e',
                    border: '1px solid #333',
                    borderRadius: 8,
                  }}
                  labelStyle={{ color: '#e0e0e0' }}
                />
                <Bar dataKey="messages" fill="#7c3aed" radius={[4, 4, 0, 0]} />
              </BarChart>
            </ResponsiveContainer>
          </div>
        )}

        <div className="space-y-4">
          <div className="rounded-lg border border-border-subtle bg-surface/50 p-6">
            <h3 className="mb-4 text-lg font-semibold">Models</h3>
            <div className="space-y-2">
              {metrics?.models.map((m) => (
                <div key={m.model} className="flex items-center justify-between text-sm">
                  <span className="text-text-secondary">{m.model}</span>
                  <span className="font-medium text-accent">{m.count}</span>
                </div>
              ))}
              {(metrics?.models.length ?? 0) === 0 && (
                <p className="text-sm text-text-muted">No data yet.</p>
              )}
            </div>
          </div>

          <div className="rounded-lg border border-border-subtle bg-surface/50 p-6">
            <h3 className="mb-4 text-lg font-semibold">Tool Usage</h3>
            <div className="space-y-2">
              {metrics?.tool_usage.map((t) => (
                <div key={t.name} className="flex items-center justify-between text-sm">
                  <span className="text-text-secondary">{t.name}</span>
                  <span className="flex items-center gap-2">
                    <span className="font-medium text-accent">{t.calls}</span>
                    {t.errors > 0 && (
                      <span className="text-xs text-red-400">{t.errors} err</span>
                    )}
                  </span>
                </div>
              ))}
              {(metrics?.tool_usage.length ?? 0) === 0 && (
                <p className="text-sm text-text-muted">No data yet.</p>
              )}
            </div>
          </div>

          <div className="rounded-lg border border-border-subtle bg-surface/50 p-4">
            <div className="flex items-center justify-between text-sm">
              <span className="text-text-secondary">Tool Success Rate</span>
              <span className="font-medium text-accent">
                {(successRate * 100).toFixed(1)}%
              </span>
            </div>
            <div className="mt-2 h-2 rounded-full bg-hover">
              <div
                className="h-2 rounded-full bg-accent transition-all"
                style={{ width: `${successRate * 100}%` }}
              />
            </div>
          </div>
        </div>
      </div>

      {/* Live Events */}
      {events.length > 0 && (
        <div className="mt-6 rounded-lg border border-border-subtle bg-surface/50 p-6">
          <div className="mb-4 flex items-center justify-between">
            <h3 className="text-lg font-semibold">Live Events</h3>
            <button
              type="button"
              onClick={clear}
              className="text-xs text-text-muted transition-colors hover:text-text-primary"
            >
              Clear
            </button>
          </div>
          <div className="max-h-48 space-y-1 overflow-y-auto">
            {events.slice(-20).map((ev, i) => (
              <div
                key={i}
                className="flex items-center gap-2 rounded px-2 py-1 text-xs hover:bg-hover"
              >
                <span className="font-mono text-accent">{ev.type}</span>
                {typeof ev.session_id === 'string' && (
                  <span className="text-text-muted">
                    {ev.session_id.slice(0, 8)}
                  </span>
                )}
                {typeof ev.tool_name === 'string' && (
                  <span className="text-text-secondary">{ev.tool_name}</span>
                )}
                {typeof ev.message === 'string' && (
                  <span className="truncate text-text-muted">
                    {ev.message.slice(0, 60)}
                  </span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}
