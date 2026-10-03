//! `minify`: the configured minifier of the resource's media type (`ssg-minify`). A media
//! type without a minifier (text, images, unknown) is an error, as in Go. A leading byte
//! order mark (dart-sass writes one before non-ASCII compressed CSS) is kept in front of the
//! minified text.

use super::{Output, PipeError, TransformEnv, text};
use crate::store::Resource;

pub(super) fn run(env: &TransformEnv, r: &Resource, input: &[u8]) -> Result<Output, PipeError> {
    let media_type = r.media_type_string();
    let Some(target) = ssg_minify::target_for(&media_type) else {
        return Err(PipeError::NoMinifier(media_type));
    };
    let text = text(input)?;
    let (bom, body) = match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    };
    let min = env.minifier.minify(target, body)?;
    Ok(Output {
        bytes: format!("{bom}{min}").into_bytes(),
        source_map: None,
    })
}
