//! Chroma's styles, converted from its XML (crate README, "Lexer and style files"), and
//! [`STYLES`], in Chroma's order (its file names, bytewise). Written by
//! `tests/it/xml2rust.rs` with the files.

use crate::style::StyleDef;

mod abap;
mod algol;
mod algol_nu;
mod arduino;
mod autumn;
mod average;
mod base16_snazzy;
mod borland;
mod bw;
mod catppuccin_frappe;
mod catppuccin_latte;
mod catppuccin_macchiato;
mod catppuccin_mocha;
mod colorful;
mod doom_one;
mod doom_one2;
mod dracula;
mod emacs;
mod evergarden;
mod friendly;
mod fruity;
mod github;
mod github_dark;
mod gruvbox;
mod gruvbox_light;
mod hr_high_contrast;
mod hrdark;
mod igor;
mod lovelace;
mod manni;
mod modus_operandi;
mod modus_vivendi;
mod monokai;
mod monokailight;
mod murphy;
mod native;
mod nord;
mod nordic;
mod onedark;
mod onesenterprise;
mod paraiso_dark;
mod paraiso_light;
mod pastie;
mod perldoc;
mod pygments;
mod rainbow_dash;
mod rose_pine;
mod rose_pine_dawn;
mod rose_pine_moon;
mod rpgle;
mod rrt;
mod solarized_dark;
mod solarized_dark256;
mod solarized_light;
mod swapoff;
mod tango;
mod tokyonight_day;
mod tokyonight_moon;
mod tokyonight_night;
mod tokyonight_storm;
mod trac;
mod vim;
mod vs;
mod vulcan;
mod witchhazel;
mod xcode;
mod xcode_dark;

/// Every one of this directory, in Chroma's order.
#[rustfmt::skip]
pub(crate) static STYLES: &[&StyleDef] = &[
    &abap::STYLE,
    &algol::STYLE,
    &algol_nu::STYLE,
    &arduino::STYLE,
    &autumn::STYLE,
    &average::STYLE,
    &base16_snazzy::STYLE,
    &borland::STYLE,
    &bw::STYLE,
    &catppuccin_frappe::STYLE,
    &catppuccin_latte::STYLE,
    &catppuccin_macchiato::STYLE,
    &catppuccin_mocha::STYLE,
    &colorful::STYLE,
    &doom_one::STYLE,
    &doom_one2::STYLE,
    &dracula::STYLE,
    &emacs::STYLE,
    &evergarden::STYLE,
    &friendly::STYLE,
    &fruity::STYLE,
    &github_dark::STYLE,
    &github::STYLE,
    &gruvbox_light::STYLE,
    &gruvbox::STYLE,
    &hr_high_contrast::STYLE,
    &hrdark::STYLE,
    &igor::STYLE,
    &lovelace::STYLE,
    &manni::STYLE,
    &modus_operandi::STYLE,
    &modus_vivendi::STYLE,
    &monokai::STYLE,
    &monokailight::STYLE,
    &murphy::STYLE,
    &native::STYLE,
    &nord::STYLE,
    &nordic::STYLE,
    &onedark::STYLE,
    &onesenterprise::STYLE,
    &paraiso_dark::STYLE,
    &paraiso_light::STYLE,
    &pastie::STYLE,
    &perldoc::STYLE,
    &pygments::STYLE,
    &rainbow_dash::STYLE,
    &rose_pine_dawn::STYLE,
    &rose_pine_moon::STYLE,
    &rose_pine::STYLE,
    &rpgle::STYLE,
    &rrt::STYLE,
    &solarized_dark::STYLE,
    &solarized_dark256::STYLE,
    &solarized_light::STYLE,
    &swapoff::STYLE,
    &tango::STYLE,
    &tokyonight_day::STYLE,
    &tokyonight_moon::STYLE,
    &tokyonight_night::STYLE,
    &tokyonight_storm::STYLE,
    &trac::STYLE,
    &vim::STYLE,
    &vs::STYLE,
    &vulcan::STYLE,
    &witchhazel::STYLE,
    &xcode_dark::STYLE,
    &xcode::STYLE,
];
