export type DocType = "pdf" | "md" | "txt" | "docx" | "epub" | "html";

export interface AppInfo {
  name: string;
  version: string;
  data_dir: string;
}

export interface DocumentRecord {
  id: string;
  content_hash: string;
  title: string | null;
  last_path: string | null;
  doc_type: DocType | string;
  file_size: number | null;
  added_at: string;
  last_opened_at: string | null;
}

export interface OpenedDocument {
  path: string;
  doc_type: string;
  title: string;
  content_hash: string;
  file_size: number;
  text: string | null;
}

export interface OpenResult {
  document: DocumentRecord;
  opened: OpenedDocument;
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

export interface ExportPreview {
  markdown: string;
  title: string;
}

/** Reflow: text offsets. PDF: page-space geometry. */
export interface Annotation {
  id: string;
  document_id: string;
  ann_type: string;
  page: number | null;
  position_json: string;
  content: string | null;
  color: string | null;
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
  /** freehand stroke in page-space */
  points?: PdfPoint[];
  shape?: "rect" | "ellipse" | "arrow";
}
