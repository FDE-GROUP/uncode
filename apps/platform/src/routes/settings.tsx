import { useQuery } from '@tanstack/react-query'
import { fetchApi } from '@/lib/api'
import { useTheme } from '@/themes/ThemeContext'
import type { PresetId } from '@/themes/types'
import { useEvents } from '@/hooks/useEvents'

interface SettingsData {
  data_dir: string
  version: string
  github_repo: string
  has_github_token: boolean
}

const PRESET_LABELS: Record<string, string> = {
  dark: 'Dark',
  light: 'Light',
  midnight: 'Midnight',
}

export function SettingsPage() {
  const { config, setPreset } = useTheme()
  const { connected } = useEvents()

  const { data: settings } = useQuery<SettingsData>({
    queryKey: ['settings'],
    queryFn: () => fetchApi('/api/settings'),
  })

  return (
    <div className="flex flex-1 flex-col p-6">
      <h2 className="mb-6 text-2xl font-bold">Settings</h2>

      <div className="mx-auto w-full max-w-2xl space-y-6">
        {/* Connection */}
        <section className="rounded-lg border border-border-subtle bg-surface/50 p-6">
          <h3 className="mb-4 text-lg font-semibold">Connection</h3>
          <div className="space-y-3 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-text-secondary">WebSocket</span>
              <span className="flex items-center gap-1.5">
                <span
                  className={`inline-block h-2 w-2 rounded-full ${
                    connected ? 'bg-green-400' : 'bg-red-400'
                  }`}
                />
                <span className={connected ? 'text-green-400' : 'text-red-400'}>
                  {connected ? 'Connected' : 'Disconnected'}
                </span>
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-text-secondary">GitHub Repo</span>
              <span className="font-mono text-xs text-accent">
                {settings?.github_repo ?? '—'}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-text-secondary">GitHub Token</span>
              <span className={settings?.has_github_token ? 'text-green-400' : 'text-text-muted'}>
                {settings?.has_github_token ? 'Configured' : 'Not set'}
              </span>
            </div>
          </div>
        </section>

        {/* Theme */}
        <section className="rounded-lg border border-border-subtle bg-surface/50 p-6">
          <h3 className="mb-4 text-lg font-semibold">Theme</h3>
          <div className="flex gap-2">
            {Object.entries(PRESET_LABELS).map(([id, label]) => (
              <button
                key={id}
                type="button"
                onClick={() => setPreset(id as PresetId)}
                className={`rounded-lg border px-4 py-2 text-sm font-medium transition-colors ${
                  config.presetId === id
                    ? 'border-accent bg-accent/10 text-accent'
                    : 'border-border-subtle text-text-secondary hover:border-border-default hover:text-text-primary'
                }`}
              >
                {label}
              </button>
            ))}
          </div>
        </section>

        {/* Data */}
        <section className="rounded-lg border border-border-subtle bg-surface/50 p-6">
          <h3 className="mb-4 text-lg font-semibold">Data</h3>
          <div className="space-y-3 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-text-secondary">Session Directory</span>
              <span className="max-w-xs truncate font-mono text-xs text-accent">
                {settings?.data_dir ?? '—'}
              </span>
            </div>
          </div>
        </section>

        {/* About */}
        <section className="rounded-lg border border-border-subtle bg-surface/50 p-6">
          <h3 className="mb-4 text-lg font-semibold">About</h3>
          <div className="space-y-3 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-text-secondary">Version</span>
              <span className="font-mono text-xs text-accent">
                v{settings?.version ?? '—'}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-text-secondary">Project</span>
              <a
                href="https://github.com/FDE-GROUP/uncode"
                target="_blank"
                rel="noreferrer"
                className="text-xs text-accent hover:underline"
              >
                FDE-GROUP/uncode
              </a>
            </div>
          </div>
        </section>
      </div>
    </div>
  )
}
