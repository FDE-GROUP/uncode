import { useNavigate } from '@tanstack/react-router'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useState, useCallback } from 'react'
import { MessageSquare, Search, ArrowUpDown } from '@/lib/lucide-icons'
import { fetchApi } from '@/lib/api'
import type { SessionListResponse } from '../types'

type SortKey = 'updated_at' | 'created_at' | 'message_count'

const SORT_OPTIONS: { value: SortKey; label: string }[] = [
  { value: 'updated_at', label: 'Last Updated' },
  { value: 'created_at', label: 'Created' },
  { value: 'message_count', label: 'Messages' },
]

function relativeTime(iso: string): string {
  const now = Date.now()
  const then = new Date(iso).getTime()
  const diff = now - then
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'just now'
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  return new Date(iso).toLocaleDateString()
}

export function SessionsPage() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [search, setSearch] = useState('')
  const [sort, setSort] = useState<SortKey>('updated_at')

  const { data, isLoading } = useQuery<SessionListResponse>({
    queryKey: ['sessions', search, sort],
    queryFn: () =>
      fetchApi(
        `/api/sessions?search=${encodeURIComponent(search)}&sort=${sort}&order=desc&limit=100`,
      ),
  })

  const handleSearch = useCallback(
    (value: string) => {
      setSearch(value)
      queryClient.invalidateQueries({ queryKey: ['sessions'] })
    },
    [queryClient],
  )

  const sessions = data?.items ?? []

  return (
    <div className="flex flex-1 flex-col p-6">
      <div className="mb-6 flex items-center justify-between">
        <h2 className="text-2xl font-bold">Sessions</h2>
        <span className="text-sm text-text-muted">
          {data?.total ?? 0} session{data?.total !== 1 ? 's' : ''}
        </span>
      </div>

      {/* Search + Sort */}
      <div className="mb-6 flex items-center gap-3">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-3 h-4 w-4 -translate-y-1/2 text-text-muted" />
          <input
            type="text"
            placeholder="Search by title, model, or ID..."
            value={search}
            onChange={(e) => handleSearch(e.target.value)}
            className="w-full rounded-lg border border-border-subtle bg-surface/50 py-2 pr-4 pl-10 text-sm text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-accent"
          />
        </div>
        <div className="relative">
          <ArrowUpDown className="pointer-events-none absolute top-1/2 left-3 h-3.5 w-3.5 -translate-y-1/2 text-text-muted" />
          <select
            value={sort}
            onChange={(e) => setSort(e.target.value as SortKey)}
            className="appearance-none rounded-lg border border-border-subtle bg-surface/50 py-2 pr-8 pl-9 text-sm text-text-primary outline-none transition-colors focus:border-accent"
          >
            {SORT_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </select>
        </div>
      </div>

      {isLoading && <p className="text-text-muted">Loading...</p>}

      {!isLoading && sessions.length === 0 && (
        <div className="flex flex-col items-center justify-center py-20">
          <MessageSquare className="mb-4 h-12 w-12 text-text-muted/40" />
          <p className="mb-1 text-lg font-medium text-text-secondary">
            {search ? 'No matching sessions' : 'No sessions yet'}
          </p>
          <p className="text-sm text-text-muted">
            {search
              ? 'Try a different search term.'
              : 'Start a conversation in the TUI to create your first session.'}
          </p>
        </div>
      )}

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
        {sessions.map((s) => (
          <button
            type="button"
            key={s.id}
            onClick={() =>
              navigate({ to: '/sessions/$id', params: { id: s.id } })
            }
            className="cursor-pointer rounded-xl border border-border-subtle bg-surface/50 p-5 text-left transition-colors hover:border-border-default"
          >
            <div className="mb-3 flex items-center justify-between">
              <div className="flex items-center gap-2 text-accent">
                <MessageSquare className="h-4 w-4" />
                <span className="text-xs font-mono text-text-muted">
                  #{s.id.slice(0, 8)}
                </span>
              </div>
              <span className="text-[10px] text-text-muted">
                {relativeTime(s.updated_at)}
              </span>
            </div>
            <h3 className="mb-2 truncate text-sm font-semibold text-text-primary">
              {s.title || `Session ${s.id.slice(0, 8)}`}
            </h3>
            <div className="flex items-center gap-2 text-xs text-text-secondary">
              <span className="rounded bg-hover px-1.5 py-0.5 font-mono text-[10px]">
                {s.model}
              </span>
              <span className="text-text-muted">|</span>
              <span>
                {s.message_count} message{s.message_count !== 1 ? 's' : ''}
              </span>
              <span className="text-text-muted">|</span>
              <span>{relativeTime(s.created_at)}</span>
            </div>
          </button>
        ))}
      </div>
    </div>
  )
}
