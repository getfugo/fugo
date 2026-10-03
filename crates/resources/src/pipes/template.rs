//! `execute_as_template`: a resource's content executed as a template, published at a target
//! path. The template engine is the render layer's; this module defines the seam.

use ssg_base::ResourceId;

use super::{PipeError, text};
use crate::store::{CallSite, ResourceError, ResourceStore};

/// Executes template source text (implemented by the render layer with Tera). The executor
/// holds the template's context (the call's `data` among it) in the render layer's own values,
/// so a page passed as `data` is not converted for every call.
pub trait TemplateExecutor {
    /// Executes `source` as a template named `name` with the executor's context.
    ///
    /// # Errors
    /// A template that does not parse or fails to execute (the message is reported).
    fn execute(&self, name: &str, source: &str) -> Result<String, String>;
}

impl ResourceStore {
    /// `resources.ExecuteAsTemplate`: the content of `id` executed by `executor`, as a resource
    /// at `target` (a named target: see [`ResourceStore::from_template_output`] for its
    /// identity; the media type comes from the target).
    ///
    /// # Errors
    /// Content that is not UTF-8, a failing template, or a target conflict.
    pub fn execute_as_template(
        &self,
        id: ResourceId,
        target: &str,
        executor: &dyn TemplateExecutor,
        call: &CallSite,
    ) -> Result<ResourceId, ResourceError> {
        let r = self.resource(id);
        let fail = |e: PipeError| ResourceError::Pipe {
            resource: r.name.clone(),
            transform: "execute_as_template",
            source: Box::new(e),
        };
        let content = self.content(id)?;
        let source = text(&content).map_err(fail)?;
        let output = executor
            .execute(r.link.as_str().trim_start_matches('/'), source)
            .map_err(|e| fail(PipeError::Template(e)))?;
        self.from_template_output(target, output, call)
    }
}
