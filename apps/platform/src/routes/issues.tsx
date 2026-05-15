import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { fetchApi } from '@/lib/api'

type IssueState = 'open' | 'closed' | 'all'

interface GitHubIssue {
  id: number
  number: number
  title: string
  state: string
  html_url: string
  labels: Array<{ name: string; color: string }>
  created_at: string
  updated_at: string
  user?: { login: string }
}

const STATE_OPTIONS: { value: IssueState; label: string }[] = [
  { value: 'open', label: 'Open' },
  { value: 'closed', label: 'Closed' },
  { value: 'all', label: 'All' },
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

export function IssuesPage() {
  const [state, setState] = useState<IssueState>('open')

  const { data: issues, isLoading, error } = useQuery<GitHubIssue[]>({
    queryKey: ['issues', state],
    queryFn: () => fetchApi(`/api/issues?state=${state}&per_page=30`),
  })

  return (
    <div className="flex flex-1 flex-col p-6">
      <div className="mb-6 flex items-center justify-between">
        <h2 className="text-2xl font-bold">GitHub Issues</h2>
        <div className="flex gap-1 rounded-lg border border-border-subtle bg-surface/50 p-0.5">
          {STATE_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => setState(opt.value)}
              className={`rounded-md px-3 py-1 text-xs font-medium transition-colors ${
                state === opt.value
                  ? 'bg-accent text-root'
                  : 'text-text-secondary hover:text-text-primary'
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>

      {isLoading && <p className="text-text-muted">Loading...</p>}

      {error && (
        <div className="rounded-lg border border-red-400/30 bg-red-400/5 p-4 text-sm text-red-400">
          Failed to load issues. Please check the backend connection.
        </div>
      )}

      <div className="space-y-2">
        {issues?.map((issue) => (
          <a
            key={issue.id}
            href={issue.html_url}
            target="_blank"
            rel="noreferrer"
            className="block rounded-lg border border-border-subtle bg-surface/50 p-4 transition-colors hover:border-accent"
          >
            <div className="mb-1 flex items-center gap-2">
              <span
                className={`h-2 w-2 rounded-full ${
                  issue.state === 'open' ? 'bg-green-400' : 'bg-purple-400'
                }`}
              />
              <span className="font-medium text-accent">#{issue.number}</span>
              <span className="flex-1 text-text-primary">{issue.title}</span>
            </div>
            <div className="ml-4 flex items-center gap-3 text-xs text-text-muted">
              {issue.labels?.map((l) => (
                <span
                  key={l.name}
                  className="inline-block rounded border border-border-subtle px-2 py-0.5"
                  style={{
                    color: `#${l.color}`,
                    borderColor: `#${l.color}40`,
                  }}
                >
                  {l.name}
                </span>
              ))}
              {issue.user && <span>@{issue.user.login}</span>}
              <span>updated {relativeTime(issue.updated_at)}</span>
            </div>
          </a>
        ))}
      </div>

      {!isLoading && !error && issues?.length === 0 && (
        <div className="flex flex-col items-center py-16">
          <p className="text-text-muted">
            No {state === 'all' ? '' : state} issues found.
          </p>
        </div>
      )}
    </div>
  )
}
