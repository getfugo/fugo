//! Resource views (REWRITE_PLAN.md §2.5, §3.4): `page.resources` of every page, and the value
//! of any store resource for site functions.
//!
//! **Pending results.** A transform result's links, name and media type are final when it is
//! registered, so its view is built without computing it. The one exception is a `fingerprint`
//! of a pending resource (`to_css | fingerprint`): its links are provisional and it has no
//! integrity yet. Its view carries post-process placeholders for `rel_permalink`,
//! `permalink` and `data.integrity` (the resource is registered with
//! [`ResourceStore::post_process`]), which keeps Go's laziness: the output is held and
//! patched in phase E5 (T42's recommendation).
//! [`post_processed_view`] is the view of an explicit `post_process`: every field that is only
//! known in E5 is a placeholder.

use std::sync::Arc;

use ssg_base::{PageId, ResourceId, Value};
use ssg_config::media::MediaType;
use ssg_page::ResourceMetaRule;
use ssg_resources::meta::ResourceMeta;
use ssg_resources::{
    AdapterResource, Body, BundleResource, Origin, PpField, PublishPolicy, Resource, ResourceError,
    ResourceKind, ResourceStore, Transform,
};
use ssg_site::{AddedBody, AddedContent, AddedResource, Model};

use crate::cache::ViewError;
use crate::views::{MediaTypeView, ResourceDataView, ResourceView, params_value};

/// Whether `r` is a `fingerprint` whose input is not computed yet (provisional links, no
/// integrity).
fn pending_fingerprint(r: &Resource) -> bool {
    r.body == Body::Pending
        && matches!(&r.origin, Origin::Transformed { transform, .. }
            if matches!(**transform, Transform::Fingerprint(_)))
}

fn base_view(store: &ResourceStore, r: &Resource) -> ResourceView {
    let (width, height) = store
        .image_size(r)
        .map_or((None, None), |(w, h)| (Some(w), Some(h)));
    ResourceView {
        rid: r.id.raw(),
        name: r.name.clone(),
        title: r.title.clone(),
        params: params_value(&r.params),
        resource_type: r.resource_type().to_owned(),
        media_type: MediaTypeView::new(&r.media_type),
        rel_permalink: r.rel_permalink.clone(),
        permalink: r.permalink.to_string(),
        width,
        height,
        data: ResourceDataView {
            integrity: r.integrity().map(str::to_owned),
        },
        page_id: match r.kind {
            ResourceKind::Page(p) => Some(p.raw()),
            _ => None,
        },
    }
}

/// The view of store resource `id` (see the module docs for pending fingerprints).
#[must_use]
pub fn resource_view(store: &ResourceStore, id: ResourceId) -> ResourceView {
    let r = store.resource(id);
    let mut v = base_view(store, &r);
    if pending_fingerprint(&r) {
        let pp = store.post_process(id);
        v.rel_permalink = pp.placeholder(PpField::RelPermalink);
        v.permalink = pp.placeholder(PpField::Permalink);
        v.data.integrity = Some(pp.placeholder(PpField::Integrity));
    }
    v
}

/// The view of `post_process(id)`: links, integrity and media type are placeholders filled in
/// phase E5 (`resource_content` of it is the content placeholder).
#[must_use]
pub fn post_processed_view(store: &ResourceStore, id: ResourceId) -> ResourceView {
    let r = store.resource(id);
    let pp = store.post_process(id);
    let mut v = base_view(store, &r);
    v.rel_permalink = pp.placeholder(PpField::RelPermalink);
    v.permalink = pp.placeholder(PpField::Permalink);
    v.data.integrity = Some(pp.placeholder(PpField::Integrity));
    v.media_type.r#type = pp.placeholder(PpField::MediaType);
    v
}

/// The `resources` front matter of a page in `ssg-resources`' form.
fn resource_meta(rules: &[ResourceMetaRule]) -> Result<ResourceMeta, ResourceError> {
    if rules.is_empty() {
        return Ok(ResourceMeta::default());
    }
    let items = rules
        .iter()
        .map(|r| {
            let mut m = ssg_base::Map::new();
            m.insert("src", Value::string(&r.src));
            if let Some(n) = &r.name {
                m.insert("name", Value::string(n));
            }
            if let Some(t) = &r.title {
                m.insert("title", Value::string(t));
            }
            if !r.params.is_empty() {
                m.insert("params", Value::map(r.params.as_map().clone()));
            }
            Value::map(m)
        })
        .collect();
    ResourceMeta::parse(&Value::array(items))
}

/// One entry of a page's `.Resources`: its store id and view.
#[derive(Clone, Debug)]
pub struct PageResource {
    pub id: ResourceId,
    pub view: ResourceView,
}

/// `.Resources` of every page: the model's bundle files registered in `store` (a bundled
/// content page is registered too, never published, and viewed as a `page` resource), with the
/// page's `resources` front matter applied; files re-sorted by type and name when metadata
/// renamed them, bundled pages after them.
///
/// # Errors
/// Invalid `resources` front matter ([`ViewError::Resources`]).
pub fn page_resources(
    model: &Model,
    store: &ResourceStore,
) -> Result<Vec<Vec<PageResource>>, ViewError> {
    let mut registered: Vec<Option<ResourceId>> = vec![None; model.bundle_resources.len()];
    let mut out = Vec::with_capacity(model.pages.len());
    for p in &model.pages {
        if p.resources.is_empty() {
            out.push(Vec::new());
            continue;
        }
        let meta = resource_meta(&p.meta.resources).map_err(|source| ViewError::Resources {
            page: p.path(),
            source,
        })?;
        let mut files = Vec::new();
        let mut pages = Vec::new();
        for &mid in &p.resources {
            let br = &model.bundle_resources[mid];
            let sid = *registered[ssg_base::Idx::index(mid)].get_or_insert_with(|| {
                let dir = br.target_base.as_ref().map_or_else(
                    || format!("/{}", br.key.parent().unwrap_or_default().as_str()),
                    |b| b.link.to_string(),
                );
                let policy = match (br.page, br.publish) {
                    (Some(_), _) => PublishPolicy::Never,
                    (None, true) => PublishPolicy::Eager,
                    (None, false) => PublishPolicy::OnReference,
                };
                match &br.adapter {
                    Some(a) => store.register_adapter_resource(&adapter_resource(
                        a,
                        br.lang,
                        br.name.clone(),
                        dir,
                        policy,
                    )),
                    None => store.register_bundle(&BundleResource {
                        lang: br.lang,
                        file: br.file.abs.clone(),
                        name: br.name.clone(),
                        dir,
                        policy,
                    }),
                }
            });
            match br.page {
                Some(q) => pages.push(page_resource(model, store, sid, q, &meta)),
                None => {
                    let id = store.apply_meta(sid, &meta);
                    files.push(PageResource {
                        id,
                        view: resource_view(store, id),
                    });
                }
            }
        }
        if !meta.entries.is_empty() {
            files.sort_by(|a, b| {
                (&a.view.resource_type, &a.view.name).cmp(&(&b.view.resource_type, &b.view.name))
            });
        }
        files.extend(pages);
        out.push(files);
    }
    Ok(out)
}

/// The store's description of a resource a content adapter added, named `name` below the
/// link directory `dir` of its page.
fn adapter_resource(
    a: &AddedResource,
    lang: ssg_base::LangIdx,
    name: String,
    dir: String,
    policy: PublishPolicy,
) -> AdapterResource {
    let (body, media_type, place) = match &a.content {
        AddedContent::Text { text, media_type } => (
            Body::Bytes(Arc::from(text.as_bytes())),
            media_type.clone(),
            None,
        ),
        AddedContent::Resource {
            body,
            media_type,
            target,
            link,
        } => (
            match body {
                AddedBody::File(p) => Body::File(p.clone()),
                AddedBody::Bytes(b) => Body::Bytes(Arc::clone(b)),
            },
            Some(media_type.clone()),
            Some((target.clone(), link.clone())),
        ),
    };
    AdapterResource {
        lang,
        body,
        media_type,
        name,
        dir,
        place,
        display_name: a.name.clone(),
        title: a.title.clone(),
        params: a.params.clone(),
        policy,
    }
}

/// A bundled content page as a resource of its bundle: its title and params, `resources`
/// metadata applied, no links (bundled pages have none), and the media type of every page,
/// `application/octet-stream` (Go's pages are not typed by their content file).
fn page_resource(
    model: &Model,
    store: &ResourceStore,
    id: ResourceId,
    page: PageId,
    meta: &ResourceMeta,
) -> PageResource {
    let q = &model.pages[page];
    let r = store.resource(id);
    let a = meta.apply(&r.name, &q.title, q.params());
    let mut view = base_view(store, &r);
    view.name = a.name;
    view.title = a.title;
    view.params = params_value(&a.params);
    "page".clone_into(&mut view.resource_type);
    view.media_type = MediaTypeView::new(
        &MediaType::parse("application/octet-stream").expect("a valid media type"),
    );
    view.rel_permalink = String::new();
    view.permalink = String::new();
    view.page_id = Some(page.raw());
    PageResource { id, view }
}
