import { useNavigate } from '@tanstack/react-router'

const FEATURES = [
  {
    title: 'Agent Engine',
    description:
      'Autonomous AI agents that write, refactor, and debug code with tool-use capabilities.',
    icon: (
      <svg
        className="h-6 w-6"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <polyline points="16 18 22 12 16 6" />
        <polyline points="8 6 2 12 8 18" />
      </svg>
    ),
  },
  {
    title: 'Multi-Model',
    description:
      'Support for 7 LLM providers with configurable models, fallback chains, and hot-switching.',
    icon: (
      <svg
        className="h-6 w-6"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <rect x="4" y="4" width="16" height="16" rx="2" />
        <rect x="9" y="9" width="6" height="6" rx="1" />
        <line x1="9" y1="2" x2="9" y2="4" />
        <line x1="15" y1="2" x2="15" y2="4" />
        <line x1="9" y1="20" x2="9" y2="22" />
        <line x1="15" y1="20" x2="15" y2="22" />
      </svg>
    ),
  },
  {
    title: 'Session Tracking',
    description:
      'Full conversation history with token usage metrics, tool call inspection, and branching.',
    icon: (
      <svg
        className="h-6 w-6"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M12 20h9" />
        <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
      </svg>
    ),
  },
  {
    title: 'Skills System',
    description:
      'Extensible skill framework with 5 built-in skills and custom Markdown-based definitions.',
    icon: (
      <svg
        className="h-6 w-6"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
      </svg>
    ),
  },
]

export function HomePage() {
  const navigate = useNavigate()

  return (
    <div className="flex flex-1 flex-col items-center justify-center px-8 py-12">
      <div className="pointer-events-none fixed inset-0">
        <div className="absolute top-1/4 left-1/4 h-96 w-96 rounded-full bg-accent/10 blur-3xl" />
        <div className="absolute right-1/4 bottom-1/4 h-96 w-96 rounded-full bg-node-interface/10 blur-3xl" />
      </div>

      <div className="relative z-10 w-full max-w-3xl">
        <div className="mb-12 text-center">
          <h1 className="mb-4 text-4xl font-semibold tracking-tight">
            <span className="text-accent">uncode</span>
          </h1>
          <p className="mx-auto max-w-xl text-lg leading-relaxed text-text-secondary">
            AI Agent Coding System. Command autonomous agents to write, review,
            and refactor code with multi-model support.
          </p>
        </div>

        <div className="mb-10 grid grid-cols-2 gap-4">
          {FEATURES.map((f) => (
            <div
              key={f.title}
              className="rounded-xl border border-border-subtle bg-surface/50 p-5 transition-colors hover:border-border-default"
            >
              <div className="mb-3 text-accent">{f.icon}</div>
              <h3 className="mb-1 text-sm font-semibold text-text-primary">
                {f.title}
              </h3>
              <p className="text-xs leading-relaxed text-text-secondary">
                {f.description}
              </p>
            </div>
          ))}
        </div>

        <div className="flex items-center justify-center gap-3">
          <button
            type="button"
            onClick={() => navigate({ to: '/sessions' })}
            className="rounded-lg bg-accent px-6 py-2.5 text-sm font-semibold text-root transition-colors hover:brightness-110"
          >
            View Sessions
          </button>
          <button
            type="button"
            onClick={() => navigate({ to: '/dashboard' })}
            className="rounded-lg border border-border-default px-6 py-2.5 text-sm font-semibold text-text-primary transition-colors hover:bg-hover"
          >
            Dashboard
          </button>
        </div>
      </div>
    </div>
  )
}
