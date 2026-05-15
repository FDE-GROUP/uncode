import { useNavigate, useParams } from '@tanstack/react-router'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft } from 'lucide-react'
import { fetchApi } from '@/lib/api'
import type { SessionDetail } from '../types'

export function SessionDetailPage() {
  const { id } = useParams({ from: '/sessions/$id' })
  const navigate = useNavigate()
  const { data: session, isLoading } = useQuery<SessionDetail>({
    queryKey: ['session', id],
    queryFn: () => fetchApi(`/api/sessions/${id}`),
  })

  if (isLoading) {
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

  return (
    <div className="flex flex-1 flex-col overflow-y-auto p-6">
      <button
        type="button"
        onClick={() => navigate({ to: '/sessions' })}
        className="mb-4 flex cursor-pointer items-center gap-1.5 text-sm text-text-secondary transition-colors hover:text-text-primary"
      >
        <ArrowLeft className="h-4 w-4" />
        Back to Sessions
      </button>

      <h2 className="mb-2 text-2xl font-bold">
        {session.title || `Session ${session.id.slice(0, 8)}`}
      </h2>
      <p className="mb-6 text-sm text-text-secondary">
        {session.model} &nbsp;|&nbsp; {session.working_dir}
      </p>

      <div className="space-y-1">
        {session.entries?.map((entry, i) => (
          <div
            key={i}
            className={`rounded border-l-4 p-3 ${
              entry.type === 'system'
                ? 'border-accent bg-surface'
                : 'border-hover bg-elevated'
            }`}
          >
            {entry.type === 'message' && (
              <>
                <span className="text-xs text-accent">{entry.role}</span>
                {entry.content?.map((block, j) => (
                  <div key={j} className="mt-1">
                    {block.type === 'text' && (
                      <span className="text-sm text-text-primary">
                        {block.text?.slice(0, 300)}
                      </span>
                    )}
                    {block.type === 'tool_call' && (
                      <span className="text-sm text-accent-bright">
                        &nbsp;{block.name}({Object.keys(block.arguments).join(', ')})
                      </span>
                    )}
                    {block.type === 'tool_result' && (
                      <span
                        className={`text-sm ${
                          block.is_error ? 'text-red-400' : 'text-green-400'
                        }`}
                      >
                        {block.is_error ? '✗' : '✓'}{' '}
                        {block.content?.slice(0, 100)}
                      </span>
                    )}
                    {block.type === 'thinking' && (
                      <span className="text-xs italic text-text-muted">
                        thinking...
                      </span>
                    )}
                  </div>
                ))}
              </>
            )}
            {entry.type === 'system' && (
              <span className="text-xs text-text-secondary">
                &bull; {entry.event}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  )
}
