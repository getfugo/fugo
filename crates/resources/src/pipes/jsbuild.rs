//! `js_build`: rolldown through `ssg-jsbuild`, imports resolved in the assets view before
//! `node_modules`.

use std::sync::Arc;

use ssg_jsbuild::{JsBuildOptions, Source};

use super::assets::SharedAssets;
use super::{Output, PipeError, TransformEnv};
use crate::store::{Resource, ResourceStore};

pub(super) fn run(
    store: &ResourceStore,
    env: &TransformEnv,
    src: &Resource,
    o: &JsBuildOptions,
    input: &[u8],
) -> Result<Output, PipeError> {
    let builder = env.js_builder();
    let media_type = src.media_type_string();
    let source = Source {
        path: src.link.as_str(),
        media_type: &media_type,
        contents: input,
    };
    let assets = Arc::new(SharedAssets(store.cfg.vfs.clone()));
    let out = builder.build(assets, &source, o)?;
    Ok(Output {
        bytes: out.code,
        source_map: out.source_map,
    })
}
