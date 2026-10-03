//! Phases E2 and E3: the render waves (REWRITE_PLAN.md §3.3).
//!
//! A wave's jobs come from the session (`Session::wave1` per language, `Session::wave2`).
//! Before anything renders, every job's target is known (`Session::target`) and collisions
//! are resolved: a page (or pager) beats an alias, and among jobs of the same class the later
//! [`JobOrder`] wins. A job whose target an earlier wave already claimed is compared with that
//! file's job the same way (the language sub-waves run in language order, so among equals a
//! later language wins). Each collision is reported as a warning and in
//! [`BuildReport::collisions`].
//!
//! Losing jobs are still rendered, but their outputs are dropped: rendering is pure except for
//! what it records (the first `paginator()` call, `defer` keys), and Go renders every page,
//! so a losing list page still gets its pagers (colliding term directories, a page
//! whose `url` is `/`), which then compete for their own targets in wave 2.
//!
//! The jobs render on the render pool; each winning output goes straight to
//! `Publisher::emit` from its worker (the publisher's stats and URL tokens are sorted sets,
//! and no two published outputs share a path). Errors are reported in job order.

use std::collections::BTreeMap;

use rayon::prelude::*;
use ssg_base::diag::Diagnostic;
use ssg_base::paths::OutputPath;
use ssg_base::{FormatId, PageId};
use ssg_publish::{Emitted, Publisher};
use ssg_render::{Job, JobOrder, Session};

use crate::structure::Recorder;
use crate::{BuildError, BuildReport, Collision, RenderPool};

/// Which kind of job wrote a file: on a collision a page beats an alias.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Alias,
    Page,
}

impl Class {
    fn of(job: &Job) -> Self {
        match job {
            Job::Alias(_) | Job::PagerAlias { .. } | Job::LanguageRedirect => Self::Alias,
            Job::Page { .. } | Job::Pager { .. } | Job::Standalone { .. } => Self::Page,
        }
    }
}

/// The rank of a job for a target: class first, then order.
type Rank = (Class, JobOrder);

/// The files the waves have claimed so far, and the collisions found.
#[derive(Debug, Default)]
pub(crate) struct Written {
    files: BTreeMap<OutputPath, Rank>,
    collisions: Vec<Collision>,
}

impl Written {
    /// The collisions, by path then loser.
    pub(crate) fn into_collisions(mut self) -> Vec<Collision> {
        self.collisions
            .sort_by(|a, b| (&a.path, a.loser).cmp(&(&b.path, b.loser)));
        self.collisions
    }

    fn collide(
        &mut self,
        session: &Session,
        path: &OutputPath,
        won: &(Rank, String),
        lost: &(Rank, String),
    ) {
        self.collisions.push(Collision {
            path: path.clone(),
            winner: won.0.1,
            loser: lost.0.1,
        });
        session.diagnostics().push(
            Diagnostic::warning(format!("{path}: {} wins over {}", won.1, lost.1))
                .with_id("target-collision"),
        );
    }
}

/// A job of a wave: rendered always, published when it won its target.
struct Planned<'j> {
    job: &'j Job,
    publish: bool,
}

/// What a collision warning says about a job.
fn describe(session: &Session, job: &Job) -> String {
    let model = session.model();
    let page = |p: PageId, f: FormatId| {
        format!(
            "{} ({})",
            model.pages[p].key.to_path(),
            model.config.output_formats.get(f).name
        )
    };
    match *job {
        Job::Alias(ref a) => format!("alias of {}", page(a.to, a.format)),
        Job::Page { page: p, format } | Job::Standalone { page: p, format } => page(p, format),
        Job::Pager {
            page: p,
            format,
            number,
        } => format!("pager {number} of {}", page(p, format)),
        Job::PagerAlias { page: p, format } => format!("page 1 alias of {}", page(p, format)),
        Job::LanguageRedirect => "the language redirect".to_owned(),
    }
}

/// Resolves the targets of `jobs` against each other and against the files of earlier waves.
fn plan<'j>(
    session: &Session,
    jobs: &'j [Job],
    written: &mut Written,
) -> Result<Vec<Planned<'j>>, BuildError> {
    // Per target: the winning job's index and rank.
    let mut winners: BTreeMap<OutputPath, (usize, Rank)> = BTreeMap::new();
    for (i, job) in jobs.iter().enumerate() {
        let Some(target) = session.target(job)? else {
            continue;
        };
        let rank = (Class::of(job), session.order(job));
        let earlier = winners
            .get(&target)
            .map(|&(j, r)| (r, describe(session, &jobs[j])))
            .or_else(|| {
                written
                    .files
                    .get(&target)
                    .map(|&r| (r, "the file of an earlier wave".to_owned()))
            });
        match earlier {
            None => {
                winners.insert(target, (i, rank));
            }
            Some(prev) if prev.0 > rank => {
                written.collide(session, &target, &prev, &(rank, describe(session, job)));
            }
            Some(prev) => {
                written.collide(session, &target, &(rank, describe(session, job)), &prev);
                winners.insert(target, (i, rank));
            }
        }
    }
    let mut publish = vec![false; jobs.len()];
    for (target, (i, rank)) in winners {
        publish[i] = true;
        written.files.insert(target, rank);
    }
    let mut planned: Vec<Planned<'j>> = jobs
        .iter()
        .zip(publish)
        .map(|(job, publish)| Planned { job, publish })
        .collect();
    planned.sort_by_key(|p| session.order(p.job));
    Ok(planned)
}

/// Plans, renders and publishes one wave; `structure` records every job (the structure dump).
pub(crate) fn run(
    session: &Session,
    publisher: &Publisher,
    pool: &RenderPool,
    jobs: &[Job],
    written: &mut Written,
    report: &mut BuildReport,
    mut structure: Option<&mut Recorder>,
) -> Result<(), BuildError> {
    let planned = plan(session, jobs, written)?;
    let results: Vec<Result<(usize, bool), BuildError>> = pool.run(|| {
        planned
            .par_iter()
            .map(|p| {
                let outputs = session.render_job(p.job)?;
                let wrote = outputs.iter().any(|o| !o.text.is_empty());
                if !p.publish {
                    return Ok((0, wrote));
                }
                let mut published = 0;
                let alias = Class::of(p.job) == Class::Alias;
                for o in outputs {
                    let emitted = publisher.emit(ssg_publish::Output {
                        path: o.path,
                        text: o.text,
                        format: o.format,
                        lang: o.lang,
                        alias,
                    })?;
                    if emitted != Emitted::Empty {
                        published += 1;
                    }
                }
                Ok((published, wrote))
            })
            .collect()
    });
    for (p, r) in planned.iter().zip(results) {
        let (n, wrote) = r?;
        report.outputs += n;
        if Class::of(p.job) == Class::Alias {
            report.aliases += n;
        }
        if let Some(s) = structure.as_deref_mut() {
            s.job(session, p.job, wrote)?;
        }
    }
    Ok(())
}
