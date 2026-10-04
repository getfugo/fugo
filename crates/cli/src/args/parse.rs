//! Reading the command line before clap: the command first, flag values, booleans as Go reads them,
//! clocks and poll intervals.

use super::*;

/// The arguments (the program name first) with the command moved before the flags. The Go
/// build's command line (cobra) finds the command wherever it is among the arguments and reads
/// every flag as the command's, so `<bin> -s site server` is `<bin> server -s site` and
/// `<bin> -e production templates check` is `<bin> templates check -e production`. A
/// flag's value stays with it (`<bin> -e server` builds with the environment `server`);
/// anything this does not recognise is left where it is, for clap to report.
///
/// A boolean flag with an explicit value, which cobra (pflag) takes and clap's `SetTrue` flags
/// do not, is rewritten: `--quiet=true` (or `1`, `t`, `TRUE`, … as Go's `strconv.ParseBool`
/// reads it) becomes `--quiet`, and `--quiet=false` (`0`, `f`, `FALSE`, …) is dropped. Only flags
/// that set no configuration key are `SetTrue` (`--quiet`, `-M`, the server's), so
/// a dropped `=false` is the default; the others take the value themselves (`parse_bool`).
#[must_use]
pub fn command_first(mut args: Vec<OsString>) -> Vec<OsString> {
    let root = Cli::command();
    let mut cmd = &root;
    let mut next = 1;
    let mut i = 1;
    while i < args.len() {
        let Some(arg) = args[i].to_str() else { break };
        if arg == "--" {
            break;
        }
        if let Some((flag, on)) = explicit_bool(&root, arg) {
            if on {
                args[i] = flag.into();
                i += 1;
            } else {
                args.remove(i);
            }
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            if !long.contains('=') && takes_value(&root, &|a| has_long(a, long)) {
                i += 1;
            }
        } else if let Some(shorts) = arg.strip_prefix('-').filter(|s| !s.is_empty()) {
            // A cluster such as `-DEs dir` or `-sdir`: a short that takes a value ends it, and
            // so does `=`, which starts a value (`-D=true`, `-e=x`).
            let flags = shorts.split_once('=').map_or(shorts, |(f, _)| f);
            for (at, c) in flags.char_indices() {
                if takes_value(&root, &|a| has_short(a, c)) {
                    if at + c.len_utf8() == shorts.len() {
                        i += 1;
                    }
                    break;
                }
            }
        } else if let Some(sub) = cmd.find_subcommand(arg) {
            let name = args.remove(i);
            args.insert(next, name);
            next += 1;
            cmd = sub;
        } else {
            break;
        }
        i += 1;
    }
    args
}

/// Whether the argument of `cmd` (or else of a command below it) that `is` picks takes a separate
/// value (`--append-port=false` takes it only after `=`).
pub(super) fn takes_value(cmd: &ClapCommand, is: &dyn Fn(&Arg) -> bool) -> bool {
    match cmd.get_arguments().find(|a| is(a)) {
        Some(a) => a.get_action().takes_values() && !a.is_require_equals_set(),
        None => cmd.get_subcommands().any(|c| takes_value(c, is)),
    }
}

/// A boolean flag (`ArgAction::SetTrue`) given with an explicit value, as pflag reads it
/// (`--quiet=true`, `-M=1`): the flag without the value, and the value.
/// `None` for anything else, an unknown value included (clap reports it).
pub(super) fn explicit_bool(root: &ClapCommand, arg: &str) -> Option<(String, bool)> {
    let (flag, value) = arg.split_once('=')?;
    let found = if let Some(long) = flag.strip_prefix("--") {
        find_arg(root, &|a| has_long(a, long))
    } else {
        let mut shorts = flag.strip_prefix('-')?.chars();
        match (shorts.next(), shorts.next()) {
            (Some(c), None) => find_arg(root, &|a| has_short(a, c)),
            _ => None,
        }
    }?;
    if !matches!(found.get_action(), ArgAction::SetTrue) {
        return None;
    }
    Some((flag.to_owned(), parse_bool(value).ok()?))
}

/// A boolean flag's explicit value as pflag reads it (Go's `strconv.ParseBool`).
pub(super) fn parse_bool(s: &str) -> Result<bool, String> {
    match s {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(false),
        _ => Err(
            "must be true or false (also 1, 0, t, f, T, F, TRUE, FALSE, True, False)".to_owned(),
        ),
    }
}

/// The argument of `cmd` (or else of a command below it) that `is` picks.
pub(super) fn find_arg<'a>(cmd: &'a ClapCommand, is: &dyn Fn(&Arg) -> bool) -> Option<&'a Arg> {
    cmd.get_arguments()
        .find(|a| is(a))
        .or_else(|| cmd.get_subcommands().find_map(|c| find_arg(c, is)))
}

pub(super) fn has_long(a: &Arg, name: &str) -> bool {
    a.get_long() == Some(name) || a.get_all_aliases().is_some_and(|v| v.contains(&name))
}

pub(super) fn has_short(a: &Arg, c: char) -> bool {
    a.get_short() == Some(c) || a.get_all_short_aliases().is_some_and(|v| v.contains(&c))
}

pub(super) fn parse_clock(s: &str) -> Result<jiff::Timestamp, String> {
    s.parse::<jiff::Timestamp>()
        .map_err(|e| format!("not an RFC 3339 time with an offset: {e}"))
}

/// A poll interval: a positive number of milliseconds or a duration (`700ms`, `1s`), as Go's
/// `--poll` reads it.
pub(super) fn parse_poll(s: &str) -> Result<Duration, String> {
    let d = match s.trim().parse::<u64>() {
        Ok(ms) => Duration::from_millis(ms),
        Err(_) => ssg_config::duration::parse(s)
            .ok()
            .filter(|d| !d.negative)
            .map(|d| d.duration)
            .ok_or_else(|| format!("{s:?} is not an interval (such as 700ms or 1s)"))?,
    };
    if d.is_zero() {
        return Err("the interval must be longer than 0".to_owned());
    }
    Ok(d)
}
