export type DocType = "pdf" | "md" | "txt" | "docx" | "epub" | "html" | "tex";

export interface AppInfo {
  name: string;
  version: string;
  data_dir: string;
}

export type DiagnosticComponentState =
  | "available"
  | "unavailable"
  | "not_configured"
  | "not_checked";

export interface DiagnosticComponentStatus {
  state: DiagnosticComponentState;
  version: string | null;
  message: string;
}

export interface DiagnosticEvent {
  timestamp: string;
  level: string;
  category: string;
  message: string;
}

export interface DiagnosticStorageMetric {
  bytes: number;
  files: number;
  accessible: boolean;
}

export interface DiagnosticReport {
  format_version: number;
  generated_at: string;
  app_version: string;
  database_schema_version: number | null;
  supported_schema_version: number;
  integrity: { ok: boolean; message: string };
  last_successful_backup: {
    kind: BackupKind;
    created_at: string;
    size_bytes: number;
    schema_version: number;
  } | null;
  storage: {
    database: DiagnosticStorageMetric;
    attachments: DiagnosticStorageMetric;
    media: DiagnosticStorageMetric;
    search_index: DiagnosticStorageMetric;
  };
  components: {
    pdf_worker: DiagnosticComponentStatus;
    ocr: DiagnosticComponentStatus;
    djvu: DiagnosticComponentStatus;
    chroma_tsvet: DiagnosticComponentStatus;
  };
  recent_errors: DiagnosticEvent[];
}

export interface DiagnosticArchiveResult {
  file_name: string;
  size_bytes: number;
  generated_at: string;
}

export interface DocumentRecord {
  id: string;
  content_hash: string;
  sha256: string | null;
  title: string | null;
  last_path: string | null;
  doc_type: DocType | string;
  file_size: number | null;
  added_at: string;
  last_opened_at: string | null;
  version_count: number;
}

export interface DocumentVersion {
  id: string;
  document_id: string;
  sha256: string | null;
  legacy_hash: string | null;
  file_size: number | null;
  path: string | null;
  title: string | null;
  change_kind: string;
  observed_at: string;
}

export interface OpenedDocument {
  path: string;
  doc_type: string;
  title: string;
  content_hash: string;
  file_size: number;
  text: string | null;
  /** PDF payload from backend (preferred). */
  binary_base64?: string | null;
  /** Local cache path for PDF / media root. */
  cache_path?: string | null;
}

export interface OpenResult {
  document: DocumentRecord;
  opened: OpenedDocument;
  movement_detected: boolean;
  content_changed: boolean;
  annotations_rebound: number;
  annotations_needing_review: number;
}

export interface TemplateField {
  key: string;
  label: string;
  type: "text" | "number" | "date" | "tags" | "file" | "textarea" | string;
  required?: boolean;
  default?: unknown;
}

export interface TemplateRecord {
  id: string;
  name: string;
  description: string | null;
  category: string | null;
  is_builtin: boolean;
  fields_json: string;
  body_md: string;
  default_tags_json: string | null;
  created_at: string;
  updated_at: string;
}

export interface JournalEntry {
  id: string;
  title: string;
  template_id: string | null;
  template_snapshot_json: string | null;
  body_md: string;
  fields_json: string | null;
  tags_json: string | null;
  entry_date: string;
  created_at: string;
  updated_at: string;
}

export interface JournalDraftPayload {
  title: string;
  template_id: string | null;
  body_md: string;
  fields: Record<string, string>;
  tags: string;
  entry_date: string;
}

export interface TemplateEditorDraftPayload {
  name: string;
  body_md: string;
  fields_json: string;
}

export interface JournalDraft<T = JournalDraftPayload> {
  draft_key: string;
  entry_id: string | null;
  payload: T;
  base_updated_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface ExportPreview {
  markdown: string;
  title: string;
}

export interface Annotation {
  id: string;
  document_id: string;
  ann_type: string;
  page: number | null;
  position_json: string;
  content: string | null;
  color: string | null;
  selected_text: string | null;
  context_before: string | null;
  context_after: string | null;
  anchor_status: "attached" | "rebound" | "needs_review";
  source_sha256: string | null;
  created_at: string;
  updated_at: string;
}

export interface ReflowPosition {
  start_offset: number;
  end_offset: number;
  quote?: string;
}

export interface PdfRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface PdfPoint {
  x: number;
  y: number;
}

export interface PdfPosition {
  page: number;
  rects?: PdfRect[];
  points?: PdfPoint[];
  shape?: "rect" | "ellipse" | "arrow";
}

export type BackupKind = "daily" | "manual" | "pre_migration" | "emergency" | "unknown";

export interface BackupInfo {
  id: string;
  kind: BackupKind;
  created_at: string;
  size_bytes: number;
  schema_version: number;
  file_name: string;
  readable: boolean;
  error: string | null;
}

export interface BackupRestoreResult {
  restored: BackupInfo;
  emergency: BackupInfo;
  reindexed_items: number | null;
  warning: string | null;
}

export interface WorkspaceCounts {
  settings: number;
  documents: number;
  annotations: number;
  journal_entries: number;
  journal_drafts: number;
  user_templates: number;
  user_export_templates: number;
  bibliography_items: number;
  rss_feeds: number;
  rss_items: number;
  plugins: number;
}

export interface WorkspaceExportResult {
  path: string;
  created_at: string;
  schema_version: number;
  counts: WorkspaceCounts;
  file_count: number;
  total_size: number;
  attachment_count: number;
  media_count: number;
  missing_references: string[];
}

export interface WorkspacePreview {
  token: string;
  file_name: string;
  created_at: string;
  app_version: string;
  schema_version: number;
  compatibility: "compatible" | "upgrade_required";
  counts: WorkspaceCounts;
  current_counts: WorkspaceCounts;
  file_count: number;
  total_size: number;
  attachment_count: number;
  media_count: number;
  missing_references: string[];
  requires_replacement_confirmation: boolean;
}

export interface WorkspaceImportResult {
  imported_counts: WorkspaceCounts;
  emergency: BackupInfo;
  reindexed_items: number | null;
  warning: string | null;
}
