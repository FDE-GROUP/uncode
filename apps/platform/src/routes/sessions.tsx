import { useNavigate } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { MessageSquare } from '@/lib/lucide-icons'
import { fetchApi } from '@/lib/api'
import type { SessionSummary } from '../types'

export function SessionsPage() {
  const navigate = useNavigate()
  const { data: sessions, isLoading } = useQuery<SessionSummary[]>({
    queryKey: ['sessions'],
    queryFn: () => fetchApi('/api/sessions'),
  })

  return (
    <div className="flex flex-1 flex-col p-6">
      <h2 className="mb-6 text-2xl font-bold">Sessions</h2>

      {isLoading && (
        <p className="text-text-muted">Loading...</p>
      )}

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
        {sessions?.map((s) => (
          <button
            type="button"
            key={s.id}
            onClick={() =>
              navigate({ to: '/sessions/$id', params: { id: s.id } })
            }
            className="cursor-pointer rounded-xl border border-border-subtle bg-surface/50 p-5 text-left transition-colors hover:border-border-default"
          >
            <div className="mb-3 flex items-center gap-2 text-accent">
              <MessageSquare className="h-5 w-5" />
            </div>
            <h3 className="mb-1 truncate text-sm font-semibold text-text-primary">
              {s.title || `#${s.id.slice(0, 8)}`}
            </h3>
            <div className="flex items-center gap-2 text-xs text-text-secondary">
              <span>{s.model}</span>
              <span className="text-text-muted">|</span>
              <span>{s.message_count} messages</span>
            </div>
          </button>
        ))}
      </div>

      {!isLoading && sessions?.length === 0 && (
        <p className="text-text-muted">No sessions found.</p>
      )}
    </div>
  )
}
