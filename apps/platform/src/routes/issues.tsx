import { useQuery } from '@tanstack/react-query'

interface GitHubIssue {
  id: number
  number: number
  title: string
  state: string
  html_url: string
  labels: Array<{ name: string; color: string }>
}

export function IssuesPage() {
  const { data: issues, isLoading } = useQuery<GitHubIssue[]>({
    queryKey: ['issues'],
    queryFn: () =>
      fetch(
        'https://api.github.com/repos/FDE-GROUP/uncode/issues?state=open&per_page=20',
      ).then((r) => r.json()),
  })

  return (
    <div className="flex flex-1 flex-col p-6">
      <h2 className="mb-6 text-2xl font-bold">GitHub Issues</h2>

      {isLoading && <p className="text-text-muted">Loading...</p>}

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
              <span className="font-medium text-accent">
                #{issue.number}
              </span>
              <span className="flex-1 text-text-primary">{issue.title}</span>
            </div>
            <div className="ml-4 text-xs text-text-muted">
              {issue.labels?.map((l) => (
                <span
                  key={l.name}
                  className="mr-1 inline-block rounded border border-border-subtle px-2 py-0.5"
                  style={{
                    color: `#${l.color}`,
                    borderColor: `#${l.color}40`,
                  }}
                >
                  {l.name}
                </span>
              ))}
            </div>
          </a>
        ))}
      </div>

      {!isLoading && issues?.length === 0 && (
        <p className="text-text-muted">No open issues.</p>
      )}
    </div>
  )
}
