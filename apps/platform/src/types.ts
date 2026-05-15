export interface SessionSummary {
  id: string
  model: string
  title: string | null
  message_count: number
  working_dir: string
  created_at: string
  updated_at: string
}

export interface SessionListResponse {
  items: SessionSummary[]
  total: number
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

export interface ModelStat {
  model: string
  count: number
}

export interface ToolStat {
  name: string
  calls: number
  errors: number
}

export interface MetricsResponse {
  total_sessions: number
  total_messages: number
  total_tool_calls: number
  tool_success_rate: number
  total_input_tokens: number
  total_output_tokens: number
  avg_messages_per_session: number
  models: ModelStat[]
  recent_sessions: SessionSummary[]
  tool_usage: ToolStat[]
}

export interface SessionMetrics {
  total_messages: number
  total_tool_calls: number
  tool_errors: number
  input_tokens: number
  output_tokens: number
  duration_secs: number
  tools: ToolStat[]
  files_modified: string[]
}

export interface Suggestion {
  category: string
  severity: 'high' | 'medium' | 'low'
  title: string
  description: string
  detail: string
}

export interface SuggestionsResponse {
  suggestions: Suggestion[]
}
