// The data of the API: the content index, the signed-in person, drafts, and the page open in the
// editor.

import { type FrontMatter, type Parts } from "../codec";
import { type Limits } from "../common";

interface Lang {
  key: string;
  name: string;
  content_dir?: string;
}

export interface Taxonomy {
  plural: string;
  singular: string;
  hierarchical: boolean;
  terms: string[];
}

export interface FieldHint {
  label?: string;
  widget?: string;
  options?: string[];
  help?: string;
}

export interface Section {
  key: string;
  title: string;
  count: number;
  style: { bundle: boolean; lang_suffix: boolean; format: string; ext: string };
  keys: { key: string; kind: string }[];
}

export interface EntryFile {
  lang: string;
  path: string;
  format?: string;
  title?: string;
  /** The URL of its page as the site was last built (none for pages not built yet). */
  url?: string;
  draft?: boolean;
  /** A file of a page the editor is making (not saved yet). */
  doc?: Doc;
}

export interface Entry {
  key: string;
  section: string;
  kind: string;
  bundle: boolean;
  title: string;
  files: EntryFile[];
  resources: string[];
  isNew?: boolean;
}

/** The content index (`GET site`, `crates/cms/src/index.rs`). */
export interface Site {
  title: string;
  site_url: string;
  workflow: string;
  languages: Lang[];
  default_language: string;
  taxonomies: Taxonomy[];
  fields: Record<string, FieldHint>;
  content_dir: string | null;
  media: string | null;
  media_ref: string | null;
  upload_types: string[];
  max_upload: number;
  sections: Section[];
  entries: Entry[];
}

/** The signed-in person (`GET me`). */
export interface Me extends Limits {
  email: string;
  roles: string[];
  edit: string[];
  publish: boolean;
  workflow: string;
}

interface Author {
  name: string;
  email: string;
}

export interface Draft {
  id: string;
  entry: string;
  title: string;
  author: Author | null;
  updated?: string;
}

export interface DraftDetail {
  id: string;
  entry: string;
  title: string;
  files: { path: string; status: string; previous?: string; patch?: string }[];
  commits: { author: Author | null; subject: string }[];
  conflicts: string[];
}

/** One language's file of the open page. */
export interface Doc {
  path: string;
  /** The blob id it was loaded at (null: a new file). */
  sha: string | null;
  isNew: boolean;
  parts: Parts;
  /** The front matter as loaded, and as edited (null when it could not be read). */
  original: FrontMatter | null;
  data: FrontMatter | null;
  body: string;
  error: string | null;
  /** Edited as text. */
  raw: boolean;
  rawText: string;
  /** The text as loaded. */
  initial: string;
}

interface Upload {
  path: string;
  content: string;
}

/** The open page. */
export interface Page {
  entry: Entry;
  draft: Draft | null;
  docs: Map<string, Doc>;
  uploads: Upload[];
  deletes: Set<string>;
  lang: string;
}

export interface Change {
  path: string;
  content?: string;
  encoding?: "utf-8" | "base64";
  delete?: true;
  base?: string | null;
  /** A move: the file is the one at `from`, which the same save deletes. */
  from?: string;
}
