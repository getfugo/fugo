//! The comparison, level by level, and its summary.

use super::*;

/// L1 of both passes: a key's status is the worse of the two.
pub(super) fn compare_l1(
    r: &Side,
    c: &Side,
    cdirs: &[String],
    files: &mut Results,
    notes: &mut Vec<String>,
) -> BTreeMap<String, PassCount> {
    let l1_key = |rel: &str| {
        let n = norm_path(rel);
        cdirs
            .iter()
            .find(|d| n.starts_with(d.as_str()))
            .map_or(n, |d| format!("{d}**"))
    };
    let count = |m: &Manifest| {
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        for rel in m.files.keys() {
            *out.entry(l1_key(rel)).or_default() += 1;
        }
        out
    };
    let mut passes = BTreeMap::new();
    for (pass, rm, cm) in [
        ("minified", &r.min, &c.min),
        ("unminified", &r.unmin, &c.unmin),
    ] {
        let (Some(rm), Some(cm)) = (rm, cm) else {
            let sides = if rm.is_none() && cm.is_none() {
                "both sides"
            } else {
                "one side"
            };
            notes.push(format!("L1: no {pass} pass on {sides}"));
            continue;
        };
        let (rc, cc) = (count(rm), count(cm));
        let mut tally = PassCount {
            matched: 0,
            ref_files: rm.files.len(),
            cand_files: cm.files.len(),
            ok: 0,
            missing: 0,
            extra: 0,
        };
        for key in rc.keys().chain(cc.keys()).collect::<BTreeSet<_>>() {
            let (mut a, mut b) = (
                rc.get(key).copied().unwrap_or(0),
                cc.get(key).copied().unwrap_or(0),
            );
            if key.ends_with("/**") && a > 0 && b > 0 {
                // A collision directory: only its presence is compared.
                tally.matched += a;
                a = a.min(b);
                b = a;
            } else {
                tally.matched += a.min(b);
            }
            let detail = format!("{pass}: reference {a}, candidate {b}");
            let outcome = match a.cmp(&b) {
                std::cmp::Ordering::Equal => {
                    tally.ok += 1;
                    Outcome::ok()
                }
                std::cmp::Ordering::Greater => {
                    tally.missing += 1;
                    Outcome::new(
                        Status::Missing,
                        ["L1 missing".into()],
                        detail,
                        &json!(["missing", a, b]),
                    )
                }
                std::cmp::Ordering::Less => {
                    tally.extra += 1;
                    Outcome::new(
                        Status::Extra,
                        ["L1 extra".into()],
                        detail,
                        &json!(["extra", a, b]),
                    )
                }
            };
            let slot = files.entry(key.clone()).or_default();
            if slot
                .get(&Level::L1)
                .is_none_or(|prev| prev.status == Status::Ok && outcome.status != Status::Ok)
            {
                slot.insert(Level::L1, outcome);
            }
        }
        passes.insert(pass.to_owned(), tally);
    }
    passes
}

/// L2 and L3 of the unminified pass.
pub(super) fn compare_l2_l3(ru: &Manifest, cu: &Manifest, files: &mut Results) {
    let (rg, cg) = (by_norm(ru), by_norm(cu));
    let rfiles: HashSet<String> = ru.files.keys().map(|x| norm_path(x)).collect();
    let cfiles: HashSet<String> = cu.files.keys().map(|x| norm_path(x)).collect();
    for (n, ritems) in &rg {
        let Some(citems) = cg.get(n) else { continue };
        let l2 = |items: &[&Entry]| {
            sorted(
                items
                    .iter()
                    .filter_map(|e| e.l2.clone().map(|v| (e.kind, v)))
                    .collect(),
            )
        };
        let (rp, cp) = (l2(ritems), l2(citems));
        let mut new_dangling: BTreeSet<String> =
            citems.iter().flat_map(|e| dangling(e, &cfiles)).collect();
        for e in ritems {
            for x in dangling(e, &rfiles) {
                new_dangling.remove(&x);
            }
        }
        if !rp.is_empty() || !cp.is_empty() {
            let (mut classes, mut detail) = (Vec::new(), Vec::new());
            if rp != cp {
                let (cl, d) = match (&rp[..], &cp[..]) {
                    ([(rk, rv)], [(ck, cv)]) => l2_diff((*rk, rv), (*ck, cv)),
                    _ => (
                        vec![format!("L2 {}", ritems[0].kind.name())],
                        group_detail(&rp, &cp),
                    ),
                };
                classes.extend(cl);
                detail.push(d);
            }
            let dangling: Vec<String> = new_dangling.into_iter().collect();
            if !dangling.is_empty() {
                classes.push("L2 dangling links".into());
                detail.push(format!(
                    "dangling only in the candidate {}",
                    short(&dangling, 6)
                ));
            }
            let outcome = if classes.is_empty() {
                Outcome::ok()
            } else {
                Outcome::new(
                    Status::Diff,
                    classes,
                    detail.join("; "),
                    &json!(["L2", rp, cp, dangling]),
                )
            };
            files
                .entry(n.clone())
                .or_default()
                .insert(Level::L2, outcome);
        }
        let l3 = |items: &[&Entry]| {
            sorted(
                items
                    .iter()
                    .filter_map(|e| {
                        e.l3.as_ref()
                            .map(|l| (e.kind, l.text.sha256.clone(), l.ids.clone()))
                    })
                    .collect(),
            )
        };
        let (rp, cp) = (l3(ritems), l3(citems));
        if rp.is_empty() && cp.is_empty() {
            continue;
        }
        let outcome = if rp == cp {
            Outcome::ok()
        } else if let ([(_, rs, ri)], [(_, cs, ci)], Some(rl), Some(cl)) =
            (&rp[..], &cp[..], &ritems[0].l3, &citems[0].l3)
        {
            let (mut classes, mut parts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
            if rs != cs {
                classes.push("L3 text".into());
                parts.push(format!(
                    "text {} -> {} words, {} -> {} chars",
                    rl.text.words, cl.text.words, rl.text.len, cl.text.len
                ));
            }
            if ri != ci {
                classes.push("L3 heading ids".into());
                let (gone, new) = set_changes(ri, ci);
                parts.push(if gone.is_empty() && new.is_empty() {
                    "ids in another order".into()
                } else {
                    changes("ids", &gone, &new)
                });
            }
            Outcome::new(
                Status::Diff,
                classes,
                parts.join("; "),
                &json!(["L3", rp, cp]),
            )
        } else {
            Outcome::new(
                Status::Diff,
                [format!("L3 {}", ritems[0].kind.name())],
                group_detail(&rp, &cp),
                &json!(["L3", rp, cp]),
            )
        };
        files
            .entry(n.clone())
            .or_default()
            .insert(Level::L3, outcome);
    }
}

/// The classes and detail of two different L4 values of one file.
pub(super) fn l4_diff(
    r: &(Option<L4>, Option<String>),
    c: &(Option<L4>, Option<String>),
) -> (Vec<String>, String) {
    let (mut classes, mut parts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    match (&r.0, &c.0) {
        (Some(L4::Image { image: ri }), Some(L4::Image { image: ci })) if ri != ci => {
            classes.push("L4 image".into());
            parts.push(format!("image {ri:?} -> {ci:?}"));
        }
        (
            Some(L4::Asset {
                non_empty: rn,
                referenced: rr,
            }),
            Some(L4::Asset {
                non_empty: cn,
                referenced: cr,
            }),
        ) => {
            if rn != cn {
                classes.push("L4 css/js nonEmpty".into());
                parts.push(format!("nonEmpty {rn} -> {cn}"));
            }
            if rr != cr {
                classes.push("L4 css/js referenced".into());
                parts.push(format!("referenced {rr:?} -> {cr:?}"));
            }
        }
        (rl, cl) if rl != cl => {
            classes.push("L4 type".into());
            parts.push(format!(
                "{} -> {}",
                crate::json::line(rl),
                crate::json::line(cl)
            ));
        }
        _ => {}
    }
    if r.1 != c.1 {
        classes.push("L4 static bytes".into());
        parts.push("static file bytes differ".into());
    }
    (classes, parts.join("; "))
}

/// L4 of one pass.
pub(super) fn compare_l4(rm: &Manifest, cm: &Manifest, files: &mut Results) {
    let (rg, cg) = (by_norm(rm), by_norm(cm));
    for (n, ritems) in &rg {
        let Some(citems) = cg.get(n) else { continue };
        let is_static = ritems.iter().any(|e| e.is_static);
        let values = |items: &[&Entry]| {
            sorted(
                items
                    .iter()
                    .filter(|e| e.l4.is_some() || is_static)
                    .map(|e| {
                        (
                            e.l4.clone(),
                            if is_static { e.sha256.clone() } else { None },
                        )
                    })
                    .collect(),
            )
        };
        let (rp, cp) = (values(ritems), values(citems));
        if rp.is_empty() && cp.is_empty() {
            continue;
        }
        let outcome = if rp == cp {
            Outcome::ok()
        } else if let ([r], [c]) = (&rp[..], &cp[..]) {
            let (classes, detail) = l4_diff(r, c);
            Outcome::new(Status::Diff, classes, detail, &json!(["L4", rp, cp]))
        } else {
            Outcome::new(
                Status::Diff,
                [format!("L4 {}", ritems[0].kind.name())],
                group_detail(&rp, &cp),
                &json!(["L4", rp, cp]),
            )
        };
        files
            .entry(n.clone())
            .or_default()
            .insert(Level::L4, outcome);
    }
}

/// Compares two sides of `site`; `extra_collision_dirs` are collapsed at L1 too.
#[must_use]
pub fn compare(site: &str, r: &Side, c: &Side, extra_collision_dirs: &[String]) -> Comparison {
    let mut files = Results::new();
    let mut notes = Vec::new();
    let mut cdirs: BTreeSet<String> = BTreeSet::new();
    for s in [&r.structure, &c.structure].into_iter().flatten() {
        cdirs.extend(s.collision_dirs());
    }
    cdirs.extend(
        extra_collision_dirs
            .iter()
            .map(|d| format!("{}/", d.trim_matches('/'))),
    );
    let cdirs: Vec<String> = cdirs.into_iter().collect();
    let passes = compare_l1(r, c, &cdirs, &mut files, &mut notes);
    if let (Some(ru), Some(cu)) = (&r.unmin, &c.unmin) {
        compare_l2_l3(ru, cu, &mut files);
    } else {
        notes.push("L2, L3: no unminified pass on both sides".into());
    }
    match (&r.min, &c.min, &r.unmin, &c.unmin) {
        (Some(rm), Some(cm), ..) => compare_l4(rm, cm, &mut files),
        (None, None, Some(ru), Some(cu)) if ru.has("L4") && cu.has("L4") => {
            notes.push("L4: from the unminified pass (no minified pass on both sides)".into());
            compare_l4(ru, cu, &mut files);
        }
        _ => notes.push("L4: no minified pass on both sides".into()),
    }
    let structure = match (&r.structure, &c.structure) {
        (Some(rs), Some(cs)) => compare_structure(rs, cs),
        (rs, cs) => {
            let sides = if rs.is_none() && cs.is_none() {
                "both sides"
            } else {
                "one side"
            };
            notes.push(format!("S: no structure dump on {sides}"));
            Results::new()
        }
    };
    let a7 = match (&r.unmin, &c.unmin) {
        (Some(ru), Some(cu)) => {
            let (a7, firsts) = a7(ru, cu);
            for (n, (hunk, count)) in firsts {
                if let Some(o) = files.get_mut(&n).and_then(|f| f.get_mut(&Level::L3))
                    && o.status != Status::Ok
                {
                    let more = if count > 1 {
                        format!(" (+{} more)", count - 1)
                    } else {
                        String::new()
                    };
                    let sep = if o.detail.is_empty() { "" } else { "; " };
                    o.detail = format!("{}{sep}{hunk}{more}", o.detail);
                }
            }
            Some(a7)
        }
        _ => None,
    };
    let mut res = Comparison {
        schema: SCHEMA,
        site: site.into(),
        reference: r.name.clone(),
        cand: c.name.clone(),
        collision_dirs: cdirs,
        notes,
        passes,
        files,
        structure,
        a7,
        summary: BTreeMap::new(),
        classes: Vec::new(),
        diffs: 0,
        ratchet: None,
    };
    summarize(&mut res);
    res
}

/// Fills the per-level numbers, the difference classes and the total.
pub(super) fn summarize(res: &mut Comparison) {
    for level in Level::ALL {
        let section = if level == Level::S {
            &res.structure
        } else {
            &res.files
        };
        let mut count = LevelCount::default();
        for o in section.values().filter_map(|v| v.get(&level)) {
            count.compared += 1;
            match o.status {
                Status::Ok => count.ok += 1,
                Status::Diff => count.diff += 1,
                Status::Missing => count.missing += 1,
                Status::Extra => count.extra += 1,
            }
        }
        if level == Level::S {
            let mut kinds: BTreeMap<String, BTreeMap<Status, usize>> = BTreeMap::new();
            for (k, v) in &res.structure {
                if let Some(o) = v.get(&Level::S) {
                    let kind = k.split(' ').next().unwrap_or("").to_owned();
                    *kinds.entry(kind).or_default().entry(o.status).or_default() += 1;
                }
            }
            count.by_kind = Some(kinds);
        }
        res.diffs += count.compared - count.ok;
        res.summary.insert(level, count);
    }
    let mut classes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for section in [&res.files, &res.structure] {
        for (k, levels) in section {
            for o in levels.values() {
                for c in &o.classes {
                    classes.entry(c.clone()).or_default().push(k.clone());
                }
            }
        }
    }
    let mut ranked: Vec<(String, Vec<String>)> = classes.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    res.classes = ranked
        .into_iter()
        .map(|(class, keys)| ClassCount {
            class,
            count: keys.len(),
            examples: keys.into_iter().take(5).collect(),
        })
        .collect();
}
