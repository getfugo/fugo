//! The render scope (REWRITE_PLAN.md §2.5, §4.2): where a render happens, carried as an
//! explicit value `__nh` in every Tera context. There are no thread-locals; site functions read
//! it back with [`RenderScope::from_state`].

use serde::{Deserialize, Serialize};
use ssg_base::{FormatId, FrameId, LangIdx, PageId, TxnId};

/// The context key of the scope in every render (`__nh`).
pub const SCOPE_KEY: &str = "__nh";

/// The deepest partial / render nesting a scope allows ([`RenderScope::child`]).
pub const MAX_DEPTH: u16 = 64;

/// The build phase a render belongs to.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Phase {
    /// Shortcodes, render hooks, `markdownify` (C1).
    Content,
    /// Layout jobs, `partial()`, `execute_as_template` (E2, E3).
    Layout,
    /// `defer` templates (E5).
    Deferred,
    /// Content adapters (`_content.html`), run before the model exists: they see the Meta
    /// generation of a model of the content files.
    Adapter,
}

/// Which content rendering a page value carries: the HTML one, or the one made with the
/// `_markup/*.<format>.*` hooks of a format (only when such hooks exist).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HookVariant {
    Html,
    Format(FormatId),
}

/// A memoised stage of a page's content (the cycle check of the content phase).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stage {
    /// Shortcodes executed: the `ExpandedSource`.
    Expand,
    /// Headings and identifiers (parse only).
    Fragments,
    /// The rendered content of a hook variant.
    Content(HookVariant),
}

/// Where a render happens. Serialised into every render context under [`SCOPE_KEY`] and read
/// back by site functions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderScope {
    /// The page being rendered.
    pub page: PageId,
    pub lang: LangIdx,
    pub format: FormatId,
    /// The pager number of a paginated render (`None`: pager 1 or not paginated).
    pub pager: Option<u32>,
    pub phase: Phase,
    pub variant: HookVariant,
    /// The `partial()` frame `return_value` writes to.
    pub frame: Option<FrameId>,
    /// The page-store transaction of a content-phase computation.
    pub txn: Option<TxnId>,
    /// Partial / render nesting (at most [`MAX_DEPTH`]).
    pub depth: u16,
    /// The memo cells being computed, outermost first (cycle detection).
    pub chain: Vec<(PageId, Stage)>,
    /// Phase [`Phase::Adapter`]: the content adapter run the render belongs to (`add_page`
    /// and the adapter's store find their run by it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<u32>,
}

impl RenderScope {
    /// The scope of a layout job: depth 0, no frame, no transaction, empty chain.
    #[must_use]
    pub fn layout(page: PageId, lang: LangIdx, format: FormatId, pager: Option<u32>) -> Self {
        Self {
            page,
            lang,
            format,
            pager,
            phase: Phase::Layout,
            variant: HookVariant::Html,
            frame: None,
            txn: None,
            depth: 0,
            chain: Vec::new(),
            adapter: None,
        }
    }

    /// The scope of the render that `state` belongs to; `None` when the context has no scope
    /// (a component that did not declare `@__nh`).
    ///
    /// # Errors
    /// A `__nh` value that is not a scope.
    pub fn from_state(s: &tera::State) -> tera::TeraResult<Option<Self>> {
        match s.get::<tera::Value>(SCOPE_KEY)? {
            None => Ok(None),
            Some(v) if v.is_none() => Ok(None),
            Some(v) => Self::deserialize(v).map(Some).map_err(|e| {
                tera::Error::message(format!("`{SCOPE_KEY}` is not a render scope: {e}"))
            }),
        }
    }

    /// The scope of a nested render (`partial()`, `render_string`): same page, format and
    /// pager, depth + 1, no frame (the caller allocates one).
    #[must_use]
    pub fn child(&self) -> Self {
        Self {
            frame: None,
            depth: self.depth.saturating_add(1),
            ..self.clone()
        }
    }

    /// Whether the nesting limit is exceeded.
    #[must_use]
    pub fn too_deep(&self) -> bool {
        self.depth > MAX_DEPTH
    }

    /// The Tera value stored under [`SCOPE_KEY`].
    #[must_use]
    pub fn to_value(&self) -> tera::Value {
        tera::Value::from_serializable(self)
    }
}
