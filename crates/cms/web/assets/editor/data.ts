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

/** A key's field (`crates/cms/src/fields.rs`): as the build works it out from the content, with
 * the settings of `[cms.fields]` over it. */
export interface FieldHint {
  label?: string;
  widget?: string;
  options?: string[];
  /** A select of any number of the options (the value is a list). */
  multiple?: boolean;
  help?: string;
  /** The kind of the key's values (`string`, `number`, `list`, …, `mixed`). */
  kind?: string;
  /** Short values that pages share, to suggest as you type. */
  suggestions?: string[];
  /** No page has the key yet, only the settings. */
  unused?: boolean;
  /** The key as pages write it, when that is not in lower case (the index's keys are). */
  name?: string;
  /** The key's value on a new page, and when it is added. */
  default?: unknown;
  min?: number;
  max?: number;
  step?: number;
  required?: boolean;
  /** One value for every language: an edit goes to each language's file. */
  shared?: boolean;
  /** A table, or each table of a list, shown closed with a summary of its values. */
  collapsed?: boolean;
  /** The summary of a closed table: its values in place of `{key}`. */
  summary?: string;
}

export interface Section {
  key: string;
  title: string;
  count: number;
  /** Its folders: the section pages below it, such as the terms of a taxonomy. */
  folders: number;
  style: { bundle: boolean; lang_suffix: boolean; format: string; ext: string };
  /** The keys its pages use, the kinds of their values, and the value every page gives one. */
  keys: { key: string; kind: string; default?: unknown }[];
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
  /** The page whose draft holds this one's files: a new folder's page, saved with the first
   * page in it. */
  savedWith?: string;
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
