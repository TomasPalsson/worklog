export type WorklogReviewBlock = {
  id: number
  day: string
  jira_issue: string | null
  started_at: string
  ended_at: string
  duration_seconds: number
  description: string | null
  is_personal: boolean
  ignored_at: string | null
  tempo_worklog_id: string | null
  exported_at: string | null
  project: string | null
}

export type WorklogReview = {
  blocks: WorklogReviewBlock[]
  selected: number | undefined
  editing: 'ticket' | 'description' | undefined
  error: string | undefined
}

declare module 'claude-code' {
  interface PluginState {
    worklog: { review: WorklogReview }
  }
}
