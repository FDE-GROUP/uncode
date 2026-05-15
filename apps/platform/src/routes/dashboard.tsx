import { useQuery } from '@tanstack/react-query'
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts'
import { fetchApi } from '@/lib/api'
import type { SessionSummary } from '../types'

function MetricCard({ title, value }: { title: string; value: number }) {
  return (
    <div className="rounded-lg border border-border-subtle bg-surface/50 p-5 text-center">
      <div className="mb-2 text-sm text-text-muted">{title}</div>
      <div className="text-3xl font-bold text-accent">{value}</div>
    </div>
  )
}

export function DashboardPage() {
  const { data: sessions } = useQuery<SessionSummary[]>({
    queryKey: ['sessions'],
    queryFn: () => fetchApi('/api/sessions'),
  })

  const { data: issues } = useQuery({
    queryKey: ['issues'],
    queryFn: () =>
      fetch(
        'https://api.github.com/repos/FDE-GROUP/uncode/issues?state=open&per_page=20',
      ).then((r) => r.json()),
  })

  const chartData = sessions
    ?.slice(0, 10)
    .reverse()
    .map((s, i) => ({
      name: `#${i + 1}`,
      messages: s.message_count,
    }))

  const avgMessages = sessions?.length
    ? Math.round(
        sessions.reduce((a, s) => a + s.message_count, 0) / sessions.length,
      )
    : 0

  return (
    <div className="flex flex-1 flex-col p-6">
      <h2 className="mb-6 text-2xl font-bold">Dashboard</h2>

      <div className="mb-8 grid grid-cols-3 gap-4">
        <MetricCard title="Total Sessions" value={sessions?.length || 0} />
        <MetricCard title="Open Issues" value={issues?.length || 0} />
        <MetricCard title="Avg Messages" value={avgMessages} />
      </div>

      {chartData && (
        <div className="rounded-lg border border-border-subtle bg-surface/50 p-6">
          <h3 className="mb-4 text-lg font-semibold">Messages per Session</h3>
          <ResponsiveContainer width="100%" height={300}>
            <BarChart data={chartData}>
              <XAxis dataKey="name" stroke="#888" />
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
    </div>
  )
}
