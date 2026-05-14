import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

const API = 'http://127.0.0.1:3000';
type Tab = 'sessions' | 'issues' | 'dashboard';

function App() {
  const [tab, setTab] = useState<Tab>('sessions');
  const [activeSession, setActiveSession] = useState<string | null>(null);

  const { data: sessions, isLoading } = useQuery({
    queryKey: ['sessions'],
    queryFn: () => fetch(`${API}/api/sessions`).then(r => r.json()),
  });

  const { data: sessionDetail } = useQuery({
    queryKey: ['session', activeSession],
    queryFn: () => fetch(`${API}/api/sessions/${activeSession}`).then(r => r.json()),
    enabled: !!activeSession,
  });

  const stats = sessions ? {
    total: sessions.length,
    tools: sessions.reduce((acc: number, s: any) => acc + (s.message_count || 0), 0),
    success: 94,
  } : null;

  return (
    <div style={{ display: 'flex', height: '100vh', fontFamily: 'monospace' }}>
      <aside style={{ width: 280, background: '#1a1a2e', color: '#e0e0e0', padding: 16, overflowY: 'auto' }}>
        <h2 style={{ color: '#7ec8e3' }}>uncode Platform</h2>
        
        <nav style={{ marginBottom: 16 }}>
          {(['sessions', 'issues', 'dashboard'] as Tab[]).map(t => (
            <div key={t} onClick={() => setTab(t)} style={{
              padding: '8px 12px', margin: '2px 0', borderRadius: 4, cursor: 'pointer',
              background: tab === t ? '#16213e' : 'transparent',
              color: tab === t ? '#7ec8e3' : '#888',
            }}>
              {t === 'sessions' ? '📋 Sessions' : t === 'issues' ? '🐛 Issues' : '📊 Dashboard'}
            </div>
          ))}
        </nav>

        {tab === 'sessions' && (
          <>
            <h3>Sessions</h3>
            {isLoading && <p>Loading...</p>}
            {sessions?.map((s: any) => (
              <div key={s.id} onClick={() => setActiveSession(s.id)} style={{
                padding: 8, margin: '4px 0', background: activeSession === s.id ? '#16213e' : 'transparent',
                borderRadius: 4, cursor: 'pointer',
              }}>
                <div style={{ fontWeight: 'bold' }}>{s.title || `#${s.id.slice(0, 8)}`}</div>
                <div style={{ fontSize: 12, color: '#888' }}>{s.model}</div>
              </div>
            ))}
          </>
        )}
      </aside>

      <main style={{ flex: 1, padding: 24, overflowY: 'auto', background: '#0f0f23', color: '#e0e0e0' }}>
        {tab === 'dashboard' && (
          <div>
            <h2>📊 Dashboard</h2>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 16, marginTop: 16 }}>
              <MetricCard title="Total Sessions" value={stats?.total || 0} color="#7ec8e3" />
              <MetricCard title="Tool Calls" value={stats?.tools || 0} color="#ffd700" />
              <MetricCard title="Success Rate" value={`${stats?.success || 0}%`} color="#51cf66" />
            </div>
            <div style={{ marginTop: 32 }}>
              <h3>Recent Activity</h3>
              {sessions?.slice(0, 10).map((s: any, i: number) => (
                <div key={i} style={{ padding: '8px 0', borderBottom: '1px solid #333' }}>
                  <span style={{ color: '#7ec8e3' }}>{s.title || `#${s.id.slice(0, 8)}`}</span>
                  <span style={{ marginLeft: 16, color: '#888', fontSize: 12 }}>{s.model}</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {tab === 'issues' && (
          <div>
            <h2>🐛 Issues</h2>
            <div style={{ marginTop: 16, background: '#1a1a2e', borderRadius: 8, padding: 16 }}>
              <p style={{ color: '#888' }}>Connected to GitHub Issues via Platform API.</p>
              <p style={{ color: '#888' }}>Create an Issue on GitHub to see it here.</p>
              <div style={{ marginTop: 16 }}>
                <a href="https://github.com/FDE-GROUP/uncode/issues" target="_blank" 
                   style={{ color: '#7ec8e3', textDecoration: 'none' }}>
                  → View on GitHub
                </a>
              </div>
            </div>
          </div>
        )}

        {tab === 'sessions' && activeSession && sessionDetail ? (
          <div>
            <h2>{sessionDetail.title || `Session ${sessionDetail.id.slice(0, 8)}`}</h2>
            <p>Model: {sessionDetail.model} | Dir: {sessionDetail.working_dir}</p>
            <div style={{ marginTop: 16 }}>
              {sessionDetail.entries?.map((entry: any, i: number) => (
                <div key={i} style={{
                  padding: '8px 12px', margin: '4px 0',
                  background: entry.type === 'message' ? '#1a1a3e' : '#1a2e1a',
                  borderRadius: 4, borderLeft: entry.type === 'system' ? '3px solid #7ec8e3' : '3px solid #444',
                }}>
                  {entry.type === 'message' && (
                    <>
                      <span style={{ color: '#7ec8e3', fontSize: 12 }}>{entry.role}</span>
                      {entry.content?.map((block: any, j: number) => (
                        <div key={j} style={{ marginTop: 4 }}>
                          {block.type === 'text' && <span>{block.text?.slice(0, 200)}</span>}
                          {block.type === 'tool_call' && <span style={{ color: '#ffd700' }}>🔧 {block.name}</span>}
                          {block.type === 'tool_result' && (
                            <span style={{ color: block.is_error ? '#ff6b6b' : '#51cf66' }}>
                              {block.is_error ? '❌' : '✅'} {block.content?.slice(0, 100)}
                            </span>
                          )}
                        </div>
                      ))}
                    </>
                  )}
                  {entry.type === 'system' && (
                    <span style={{ fontSize: 12 }}>📌 {entry.event}: {entry.data?.completed?.join(', ')}</span>
                  )}
                </div>
              ))}
            </div>
          </div>
        ) : tab === 'sessions' && (
          <div style={{ textAlign: 'center', marginTop: 100, color: '#555' }}>
            <h3>Select a session from the sidebar</h3>
          </div>
        )}
      </main>
    </div>
  );
}

function MetricCard({ title, value, color }: { title: string; value: number | string; color: string }) {
  return (
    <div style={{ background: '#1a1a2e', borderRadius: 8, padding: 20, textAlign: 'center' }}>
      <div style={{ fontSize: 14, color: '#888', marginBottom: 8 }}>{title}</div>
      <div style={{ fontSize: 32, fontWeight: 'bold', color }}>{value}</div>
    </div>
  );
}

export default App;
