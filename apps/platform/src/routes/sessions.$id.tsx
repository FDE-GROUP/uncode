import { useNavigate, useParams } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft } from 'lucide-react'
import { fetchApi } from '@/lib/api'
import type { ContentBlock, SessionDetail, SessionMetrics } from '../types'

function formatDuration(secs: number): string {
  if (secs < 60) return `${Math.round(secs)}s`
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${Math.round(secs % 60)}s`
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
  return String(n)
}

function formatTime(ts: string): string {
  const d = new Date(ts)
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
}

function roleLabel(role: string | undefined): { text: string; color: string } {
  switch (role) {
    case 'user':
      return { text: 'User', color: 'text-blue-400' }
    case 'assistant':
      return { text: 'Assistant', color: 'text-accent' }
    case 'tool':
      return { text: 'Tool', color: 'text-green-400' }
    case 'system':
      return { text: 'System', color: 'text-text-muted' }
    default:
      return { text: role ?? 'unknown', color: 'text-text-muted' }
  }
}

function ContentBlockView({ block }: { block: ContentBlock }) {
  switch (block.type) {
    case 'text':
      return (
        <p className="whitespace-pre-wrap text-sm text-text-primary">
          {block.text.length > 500 ? `${block.text.slice(0, 500)}...` : block.text}
        </p>
      )
    case 'thinking':
      return (
        <div className="rounded border border-border-subtle bg-hover/50 px-3 py-2">
          <p className="text-xs italic text-text-muted">
            thinking: {block.text.length > 200 ? `${block.text.slice(0, 200)}...` : block.text}
          </p>
        </div>
      )
    case 'tool_call':
      return (
        <div className="rounded border border-accent/30 bg-accent/5 px-3 py-2">
          <div className="flex items-center gap-2 text-sm">
            <span className="text-accent">&#9881;</span>
            <span className="font-medium text-accent">{block.name}</span>
            <span className="text-text-muted">
              ({Object.keys(block.arguments).join(', ')})
            </span>
          </div>
          {typeof block.arguments.path === 'string' && (
            <p className="mt-1 text-xs text-text-secondary">{block.arguments.path}</p>
          )}
        </div>
      )
    case 'tool_result':
      return (
        <div
          className={`rounded border px-3 py-2 ${
            block.is_error
              ? 'border-red-400/30 bg-red-400/5'
              : 'border-green-400/30 bg-green-400/5'
          }`}
        >
          <span className={block.is_error ? 'text-red-400' : 'text-green-400'}>
            {block.is_error ? '✗' : '✓'}
          </span>
          <span className="ml-1.5 text-xs text-text-secondary">
            {block.content.length > 150
              ? `${block.content.slice(0, 150)}...`
              : block.content}
          </span>
        </div>
      )
    case 'image':
      return <span className="text-xs text-text-muted">[image]</span>
  }
}

function MetricPill({ label, value }: { label: string; value: string }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full bg-surface px-3 py-1 text-xs">
      <span className="text-text-muted">{label}</span>
      <span className="font-medium text-accent">{value}</span>
    </span>
  )
}

export function SessionDetailPage() {
  const { id } = useParams({ from: '/sessions/$id' })
  const navigate = useNavigate()

  const { data: session, isLoading: sessionLoading } = useQuery<SessionDetail>({
    queryKey: ['session', id],
    queryFn: () => fetchApi(`/api/sessions/${id}`),
  })

  const { data: metrics } = useQuery<SessionMetrics>({
    queryKey: ['session-metrics', id],
    queryFn: () => fetchApi(`/api/sessions/${id}/metrics`),
  })

  if (sessionLoading) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-text-muted">Loading...</p>
      </div>
    )
  }

  if (!session) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <p className="text-text-muted">Session not found.</p>
      </div>
    )
  }

  const totalTokens = (metrics?.input_tokens ?? 0) + (metrics?.output_tokens ?? 0)

  return (
    <div className="flex flex-1 flex-col overflow-y-auto">
      {/* Header */}
      <div className="border-b border-border-subtle px-6 py-4">
        <button
          type="button"
          onClick={() => navigate({ to: '/sessions' })}
          className="mb-3 flex cursor-pointer items-center gap-1.5 text-sm text-text-secondary transition-colors hover:text-text-primary"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to Sessions
        </button>

        <h2 className="mb-2 text-xl font-bold">
          {session.title || `Session ${session.id.slice(0, 8)}`}
        </h2>

        <div className="flex flex-wrap gap-2">
          <MetricPill label="Model" value={session.model} />
          {metrics && (
            <>
              <MetricPill label="Duration" value={formatDuration(metrics.duration_secs)} />
              <MetricPill label="Messages" value={String(metrics.total_messages)} />
              <MetricPill label="Tool Calls" value={String(metrics.total_tool_calls)} />
              <MetricPill label="Tokens" value={formatTokens(totalTokens)} />
              <MetricPill label="Files" value={String(metrics.files_modified.length)} />
            </>
          )}
        </div>
      </div>

      {/* Timeline */}
      <div className="flex-1 px-6 py-4">
        <div className="space-y-2">
          {session.entries?.map((entry, i) => {
            if (entry.type === 'message' && entry.role && entry.content) {
              const rl = roleLabel(entry.role)
              return (
                <div key={i} className="flex gap-3">
                  <div className="flex flex-col items-center">
                    <div
                      className={`mt-1 h-2 w-2 rounded-full ${
                        entry.role === 'user'
                          ? 'bg-blue-400'
                          : entry.role === 'assistant'
                            ? 'bg-accent'
                            : 'bg-green-400'
                      }`}
                    />
                    {i < (session.entries?.length ?? 0) - 1 && (
                      <div className="flex-1 border-l border-border-subtle" />
                    )}
                  </div>
                  <div className="min-w-0 flex-1 pb-2">
                    <div className="mb-1 flex items-center gap-2">
                      <span className={`text-xs font-medium ${rl.color}`}>{rl.text}</span>
                      <span className="text-[10px] text-text-muted">
                        {formatTime(entry.timestamp)}
                      </span>
                    </div>
                    <div className="space-y-1.5">
                      {entry.content.map((block, j) => (
                        <ContentBlockView key={j} block={block} />
                      ))}
                    </div>
                  </div>
                </div>
              )
            }

            if (entry.type === 'system') {
              return (
                <div key={i} className="flex items-center gap-2 py-1">
                  <div className="h-1.5 w-1.5 rounded-full bg-text-muted" />
                  <span className="text-xs text-text-muted">
                    {formatTime(entry.timestamp)} &mdash; {entry.event}
                  </span>
                </div>
              )
            }

            if (entry.type === 'branch') {
              return (
                <div key={i} className="flex items-center gap-2 py-1">
                  <div className="h-1.5 w-1.5 rounded-full bg-node-folder" />
                  <span className="text-xs text-text-muted">branch point</span>
                </div>
              )
            }

            return null
          })}
        </div>
      </div>
    </div>
  )
}
