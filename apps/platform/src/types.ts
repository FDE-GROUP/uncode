export interface SessionSummary {
  id: string
  model: string
  title: string | null
  message_count: number
  working_dir: string
}

export interface SessionDetail {
  id: string
  model: string
  title: string | null
  working_dir: string
  entries: SessionEntry[]
}

export interface SessionEntry {
  type: 'message' | 'system' | 'branch'
  timestamp: string
  role?: string
  content?: ContentBlock[]
  usage?: TokenUsage
  event?: string
  data?: Record<string, unknown>
}

export type ContentBlock =
  | { type: 'text'; text: string }
  | { type: 'thinking'; text: string }
  | { type: 'tool_call'; id: string; name: string; arguments: Record<string, unknown> }
  | { type: 'tool_result'; tool_call_id: string; content: string; is_error: boolean }
  | { type: 'image'; mime_type: string; data: string }

export interface TokenUsage {
  input_tokens: number
  output_tokens: number
}

export interface DashboardStats {
  totalSessions: number
  totalToolCalls: number
  successRate: number
  recentSessions: SessionSummary[]
}
