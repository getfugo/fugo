//! The render scope travels through Tera contexts as `__nh` and is read back by functions;
//! `child` and the depth limit.

use ssg_base::{FormatId, FrameId, LangIdx, PageId};
use ssg_view::{HookVariant, Phase, RenderScope, SCOPE_KEY, Stage};

fn scope() -> RenderScope {
    let mut s = RenderScope::layout(
        PageId::from_raw(7),
        LangIdx::from_raw(1),
        FormatId::from_raw(2),
        Some(3),
    );
    s.variant = HookVariant::Format(FormatId::from_raw(4));
    s.frame = Some(FrameId::from_raw(9));
    s.chain = vec![
        (PageId::from_raw(1), Stage::Expand),
        (PageId::from_raw(2), Stage::Content(HookVariant::Html)),
    ];
    s
}

#[test]
fn scope_round_trips_through_a_render() {
    let mut tera = tera::Tera::default();
    tera.register_function("scope_of_caller", |_: tera::Kwargs, st: &tera::State| {
        let s = RenderScope::from_state(st)?.expect("a scope");
        Ok::<_, tera::Error>(tera::Value::from(format!("{s:?}")))
    });
    tera.register_function("no_scope", |_: tera::Kwargs, st: &tera::State| {
        Ok::<_, tera::Error>(tera::Value::from(RenderScope::from_state(st)?.is_none()))
    });
    tera.add_raw_template(
        "t",
        "{{ __nh.page }}/{{ __nh.pager }}|{{ scope_of_caller() }}",
    )
    .expect("template");
    tera.add_raw_template("bare", "{{ no_scope() }}")
        .expect("template");

    let s = scope();
    let mut ctx = tera::Context::new();
    ctx.insert_value(SCOPE_KEY, s.to_value());
    let out = tera.render("t", &ctx).expect("render");
    assert_eq!(out, format!("7/3|{s:?}"));
    assert_eq!(
        tera.render("bare", &tera::Context::new()).expect("render"),
        "true"
    );
}

#[test]
fn child_scope() {
    let s = scope();
    let c = s.child();
    assert_eq!(
        (c.page, c.format, c.pager, c.phase),
        (s.page, s.format, s.pager, Phase::Layout)
    );
    assert_eq!(c.depth, 1);
    assert_eq!(c.frame, None);
    assert!(!c.too_deep());
    let mut deep = s;
    deep.depth = ssg_view::MAX_DEPTH;
    assert!(deep.child().too_deep());
}
