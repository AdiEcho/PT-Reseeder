// ============================================================================
// API Type Definitions
// ============================================================================

// --- Auth ---

/** Response from GET /api/auth/has-user */
export interface HasUserResponse {
  has_user: boolean
}

/** Response from GET /api/auth/me */
export interface CurrentUserResponse {
  username: string
}

/** Request body for POST /api/auth/login and /api/auth/register */
export interface AuthCredentials {
  username: string
  password: string
}

// --- Sites ---

/** Site information returned by list/create/update endpoints */
export interface SiteInfo {
  id: number
  name: string
  url: string
  api_url?: string | null
  adapter_type: string
  auth_type: string
  rate_limit_interval_ms?: number | null
  rate_limit_burst?: number | null
  download_interval_ms?: number | null
  probe_status: string
  probe_detail_json?: string | null
  enabled: boolean
}

/** Detailed site view including user stats */
export interface SiteDetailData {
  site: SiteInfo
  user_stats?: SiteUserInfo | null
  probe_detail?: string | null
}

/** Request body for POST /api/sites */
export interface CreateSiteInput {
  name: string
  url: string
  api_url?: string
  adapter_type: string
  auth_type: string
  cookie?: string
  passkey?: string
  rate_limit_interval_ms?: number
  rate_limit_burst?: number
  download_interval_ms?: number
}

/** Request body for PUT /api/sites/{id} */
export interface UpdateSiteInput {
  url: string
  api_url?: string
  cookie?: string
  passkey?: string
  rate_limit_interval_ms?: number
  rate_limit_burst?: number
  download_interval_ms?: number
}

/** Request body for POST /api/sites/{id}/validate */
export interface ValidateSiteInput {
  name: string
  url: string
  api_url?: string
  adapter_type: string
  cookie?: string
  passkey?: string
}

/** Result of site validation or probe */
export interface ValidateSiteResult {
  status: string
  message: string
  detail_json?: string | null
}

/** Site definition template from GET /api/site-definitions */
export interface SiteDefinitionInfo {
  id: string
  name: string
  url: string
  api_url?: string | null
  adapter: string
  rate_limit_interval_ms?: number | null
  rate_limit_burst?: number | null
  download_interval_ms?: number | null
}

// --- Downloaders ---

/** Downloader information */
export interface DownloaderInfo {
  id: number
  name: string
  dl_type: string
  host: string
  port: number
  role: string
  auto_start: boolean
  enabled: boolean
}

/** Request body for creating/updating a downloader */
export interface CreateDownloaderInput {
  name: string
  dl_type: string
  host: string
  port: number
  username?: string
  password?: string
  role: string
  auto_start?: boolean
}

/** Request body for PATCH /api/downloaders/{id}/auto-start */
export interface ToggleAutoStartInput {
  auto_start: boolean
}

// --- Folders ---

/** Folder information */
export interface FolderInfo {
  id: number
  path: string
  scan_mode: string
  downloader_id?: number
  enabled: boolean
  last_scanned_at?: string
}

/** Request body for POST /api/folders */
export interface CreateFolderInput {
  path: string
  scan_mode: string
  downloader_id?: number
}

// --- Tasks ---

/** Task information */
export interface TaskInfo {
  id: number
  name: string
  task_type: string
  trigger_type: string
  cron_expression?: string
  status: string
  last_run_at?: string
  next_run_at?: string
  run_count: number
  site_ids: number[]
  folder_ids: number[]
  source_downloader_ids: number[]
  destination_downloader_id?: number
}

/** Request body for creating/updating a task */
export interface CreateTaskInput {
  name: string
  task_type: string
  trigger_type: string
  cron_expression?: string
  site_ids: number[]
  folder_ids: number[]
  source_downloader_ids: number[]
  destination_downloader_id?: number
}

/** Task log entry */
export interface TaskLogInfo {
  id: number
  status: string
  matched_count: number
  succeeded_count: number
  failed_count: number
  duration_ms?: number | null
  log_text?: string | null
  created_at: string
}

/** Dry run preview result */
export interface DryRunPreviewInfo {
  version: number
  would_add_count: number
  dry_run: boolean
  items: DryRunPreviewItem[]
}

/** Single item in a dry run preview / reseed run */
export interface DryRunPreviewItem {
  site_id: number
  site_name: string
  pieces_hash: string
  torrent_id?: number | null
  title?: string | null
  save_path: string
  total_size?: number | null
  detail_url?: string | null
  outcome?: string | null
}

/** Reseed run summary */
export interface ReseedRunInfo {
  log_id: number
  task_id: number
  task_name: string
  status: string
  matched_count: number
  succeeded_count: number
  failed_count: number
  duration_ms?: number | null
  dry_run: boolean
  item_count: number
  total_size?: number | null
  history_skipped_count: number
  created_at: string
}

/** Reseed run detail with items */
export interface ReseedRunDetail {
  run: ReseedRunInfo
  items: DryRunPreviewItem[]
}

// --- Repost ---

/** Repost queue entry */
export interface RepostEntry {
  id: number
  source_site_name: string
  source_torrent_id: string
  target_site_name: string
  status: string
  review_notes?: string
  submitted_at?: string
  created_at: string
}

/** Detailed repost entry response */
export interface RepostEntryResponse {
  id: number
  source_site_id: number
  source_torrent_id: string
  target_site_id: number
  raw_info_json: string
  adapted_info_json?: string
  status: string
  review_notes?: string
  submitted_at?: string
  created_at: string
}

/** Request body for POST /api/repost/queue/{id}/review */
export interface ReviewRepostInput {
  action: 'approve' | 'reject'
  notes?: string
  mapping?: Record<string, unknown>
}

/** Autofill response */
export interface AutofillResponse {
  entry_id: number
  success: boolean
  filled: string[]
  skipped: string[]
  message: string
  target_site: string
  confirmation_required: boolean
}

// --- Config ---

/** Configuration entry */
export interface ConfigEntry {
  key: string
  value: string
  updated_at?: string
}

/** Request body for PUT /api/config */
export interface UpdateConfigInput {
  key: string
  value: string
}

// --- Logs ---

/** Log file metadata */
export interface LogFileInfo {
  filename: string
  size: number
}

/** Paginated log response */
export interface LogPage {
  entries: LogEntry[]
  total_lines: number
  page: number
  page_size: number
}

/** Single log entry */
export interface LogEntry {
  timestamp: string
  level: string
  target: string
  message: string
}

/** Query params for GET /api/logs */
export interface LogQueryParams {
  filename?: string
  page?: number
  page_size?: number
  level?: string
  keyword?: string
  task_id?: number
}

// --- Dashboard ---

/** Full dashboard payload from GET /api/dashboard */
export interface DashboardData {
  overview: DashboardOverview
  site_stats: SiteReseedStats[]
  trend: TrendPoint[]
  user_info: UserInfoAggregate
}

/** Dashboard overview numbers */
export interface DashboardOverview {
  running_tasks: number
  today_success: number
  today_failed: number
  total_sites: number
  tracked_torrents: number
}

/** Per-site reseed statistics */
export interface SiteReseedStats {
  site_id: number
  site_name: string
  matched: number
  succeeded: number
  failed: number
  skipped: number
  success_rate: number
  breaker_status: string
}

/** Daily trend data point */
export interface TrendPoint {
  date: string
  succeeded: number
  failed: number
}

/** Aggregated user info across sites */
export interface UserInfoAggregate {
  total_uploaded: number
  total_downloaded: number
  total_seeding: number
  total_bonus: number
  site_count: number
  sites: SiteUserInfo[]
}

/** Per-site user info nested in the dashboard aggregate */
export interface SiteUserInfo {
  site_id: number
  site_name: string
  uploaded?: number
  downloaded?: number
  ratio?: number
  bonus?: number
  user_class?: string
  seeding_count?: number
  leeching_count?: number
  seeding_size?: number
  upload_time_seconds?: number
  fetched_at: string
}
