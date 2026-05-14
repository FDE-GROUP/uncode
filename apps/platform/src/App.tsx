import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';

const API = 'http://127.0.0.1:3000';

function App() {
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

  return (
    <div style={{ display: 'flex', height: '100vh', fontFamily: 'monospace' }}>
      <aside style={{ width: 280, background: '#1a1a2e', color: '#e0e0e0', padding: 16, overflowY: 'auto' }}>
        <h2 style={{ color: '#7ec8e3' }}>uncode Platform</h2>
        <h3>Sessions</h3>
        {isLoading && <p>Loading...</p>}
        {sessions?.map((s: any) => (
          <div
            key={s.id}
            onClick={() => setActiveSession(s.id)}
            style={{
              padding: 8,
              margin: '4px 0',
              background: activeSession === s.id ? '#16213e' : 'transparent',
              borderRadius: 4,
              cursor: 'pointer',
            }}
          >
            <div style={{ fontWeight: 'bold' }}>{s.title || `#${s.id.slice(0, 8)}`}</div>
            <div style={{ fontSize: 12, color: '#888' }}>{s.model}</div>
          </div>
        ))}
      </aside>

      <main style={{ flex: 1, padding: 24, overflowY: 'auto', background: '#0f0f23', color: '#e0e0e0' }}>
        {activeSession && sessionDetail ? (
          <div>
            <h2>{sessionDetail.title || `Session ${sessionDetail.id.slice(0, 8)}`}</h2>
            <p>Model: {sessionDetail.model} | Dir: {sessionDetail.working_dir}</p>
            <div style={{ marginTop: 16 }}>
              {sessionDetail.entries?.map((entry: any, i: number) => (
                <div key={i} style={{
                  padding: '8px 12px',
                  margin: '4px 0',
                  background: entry.type === 'message' ? '#1a1a3e' : '#1a2e1a',
                  borderRadius: 4,
                  borderLeft: entry.type === 'system' ? '3px solid #7ec8e3' : '3px solid #444',
                }}>
                  {entry.type === 'message' && (
                    <>
                      <span style={{ color: '#7ec8e3', fontSize: 12 }}>{entry.role}</span>
                      {entry.content?.map((block: any, j: number) => (
                        <div key={j} style={{ marginTop: 4 }}>
                          {block.type === 'text' && <span>{block.text?.slice(0, 200)}</span>}
                          {block.type === 'tool_call' && (
                            <span style={{ color: '#ffd700' }}>🔧 {block.name}</span>
                          )}
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
        ) : (
          <div style={{ textAlign: 'center', marginTop: 100, color: '#555' }}>
            <h3>Select a session from the sidebar</h3>
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
