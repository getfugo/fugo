//! Chroma's lexers, converted from its XML (crate README, "Lexer and style files"), and
//! [`LEXERS`], in Chroma's order (its file names, bytewise). Written by
//! `tests/it/xml2rust.rs` with the files.

use crate::chroma::defs::LexerDef;

mod abap;
mod abnf;
mod actionscript;
mod actionscript_3;
mod ada;
mod agda;
mod al;
mod alloy;
mod angular2;
mod antlr;
mod apacheconf;
mod apl;
mod applescript;
mod arangodb_aql;
mod arduino;
mod armasm;
mod atl;
mod autohotkey;
mod autoit;
mod awk;
mod ballerina;
mod bash;
mod bash_session;
mod batchfile;
mod beef;
mod bibtex;
mod bicep;
mod blitzbasic;
mod bnf;
mod bqn;
mod brainfuck;
mod c;
mod cap_n_proto;
mod cassandra_cql;
mod ceylon;
mod cfengine3;
mod cfstatement;
mod chaiscript;
mod chapel;
mod cheetah;
mod clojure;
mod cmake;
mod cobol;
mod coffeescript;
mod common_lisp;
mod coq;
mod core;
mod cpp;
mod crystal;
mod csharp;
mod css;
mod csv;
mod cue;
mod cython;
mod d;
mod dart;
mod dax;
mod desktop_entry;
mod diff;
mod django_jinja;
mod dns;
mod docker;
mod dtd;
mod dylan;
mod ebnf;
mod elixir;
mod elm;
mod emacslisp;
mod erlang;
mod factor;
mod fennel;
mod fish;
mod forth;
mod fortran;
mod fortranfixed;
mod fsharp;
mod gas;
mod gdscript;
mod gdscript3;
mod gherkin;
mod gleam;
mod glsl;
mod gnuplot;
mod go_template;
mod graphql;
mod groff;
mod groovy;
mod handlebars;
mod hare;
mod haskell;
mod hcl;
mod hexdump;
mod hlb;
mod hlsl;
mod holyc;
mod html;
mod hy;
mod idris;
mod igor;
mod ini;
mod io;
mod iscdhcpd;
mod j;
mod janet;
mod java;
mod javascript;
mod json;
mod jsonata;
mod jsonnet;
mod julia;
mod jungle;
mod kotlin;
mod lean;
mod lighttpd_configuration_file;
mod llvm;
mod lua;
mod makefile;
mod mako;
mod mason;
mod materialize_sql_dialect;
mod mathematica;
mod matlab;
mod mcfunction;
mod meson;
mod metal;
mod minizinc;
mod mlir;
mod modula_2;
mod mojo;
mod monkeyc;
mod moonscript;
mod morrowindscript;
mod myghty;
mod mysql;
mod nasm;
mod natural;
mod ndisasm;
mod newspeak;
mod nginx_configuration_file;
mod nim;
mod nix;
mod nsis;
mod objective_c;
mod objectpascal;
mod ocaml;
mod octave;
mod odin;
mod onesenterprise;
mod openedge_abl;
mod openscad;
mod org_mode;
mod pacmanconf;
mod perl;
mod php;
mod pig;
mod pkgconfig;
mod pl_pgsql;
mod plaintext;
mod plutus_core;
mod pony;
mod postgresql_sql_dialect;
mod postscript;
mod povray;
mod powerquery;
mod powershell;
mod prolog;
mod promela;
mod promql;
mod properties;
mod protocol_buffer;
mod prql;
mod psl;
mod puppet;
mod python;
mod python_2;
mod qbasic;
mod qml;
mod r;
mod racket;
mod ragel;
mod react;
mod reasonml;
mod reg;
mod rego;
mod rexx;
mod rpgle;
mod rpm_spec;
mod ruby;
mod rust;
mod sas;
mod sass;
mod scala;
mod scheme;
mod scilab;
mod scss;
mod sed;
mod sieve;
mod smali;
mod smalltalk;
mod smarty;
mod snbt;
mod snobol;
mod solidity;
mod sourcepawn;
mod sparql;
mod sql;
mod squidconf;
mod standard_ml;
mod stas;
mod stylus;
mod swift;
mod systemd;
mod systemverilog;
mod tablegen;
mod tal;
mod tasm;
mod tcl;
mod tcsh;
mod termcap;
mod terminfo;
mod terraform;
mod tex;
mod thrift;
mod toml;
mod tradingview;
mod transact_sql;
mod turing;
mod turtle;
mod twig;
mod typescript;
mod typoscript;
mod typoscriptcssdata;
mod typoscripthtmldata;
mod typst;
mod ucode;
mod v;
mod v_shell;
mod vala;
mod vb_net;
mod verilog;
mod vhdl;
mod vhs;
mod viml;
mod vue;
mod wdte;
mod webgpu_shading_language;
mod webvtt;
mod whiley;
mod xml;
mod xorg;
mod yaml;
mod yang;
mod z80_assembly;
mod zed;
mod zig;

/// Every one of this directory, in Chroma's order.
#[rustfmt::skip]
pub(crate) static LEXERS: &[&LexerDef] = &[
    &abap::LEXER,
    &abnf::LEXER,
    &actionscript::LEXER,
    &actionscript_3::LEXER,
    &ada::LEXER,
    &agda::LEXER,
    &al::LEXER,
    &alloy::LEXER,
    &angular2::LEXER,
    &antlr::LEXER,
    &apacheconf::LEXER,
    &apl::LEXER,
    &applescript::LEXER,
    &arangodb_aql::LEXER,
    &arduino::LEXER,
    &armasm::LEXER,
    &atl::LEXER,
    &autohotkey::LEXER,
    &autoit::LEXER,
    &awk::LEXER,
    &ballerina::LEXER,
    &bash::LEXER,
    &bash_session::LEXER,
    &batchfile::LEXER,
    &beef::LEXER,
    &bibtex::LEXER,
    &bicep::LEXER,
    &blitzbasic::LEXER,
    &bnf::LEXER,
    &bqn::LEXER,
    &brainfuck::LEXER,
    &csharp::LEXER,
    &cpp::LEXER,
    &c::LEXER,
    &cap_n_proto::LEXER,
    &cassandra_cql::LEXER,
    &ceylon::LEXER,
    &cfengine3::LEXER,
    &cfstatement::LEXER,
    &chaiscript::LEXER,
    &chapel::LEXER,
    &cheetah::LEXER,
    &clojure::LEXER,
    &cmake::LEXER,
    &cobol::LEXER,
    &coffeescript::LEXER,
    &common_lisp::LEXER,
    &coq::LEXER,
    &core::LEXER,
    &crystal::LEXER,
    &css::LEXER,
    &csv::LEXER,
    &cue::LEXER,
    &cython::LEXER,
    &d::LEXER,
    &dart::LEXER,
    &dax::LEXER,
    &desktop_entry::LEXER,
    &diff::LEXER,
    &django_jinja::LEXER,
    &dns::LEXER,
    &docker::LEXER,
    &dtd::LEXER,
    &dylan::LEXER,
    &ebnf::LEXER,
    &elixir::LEXER,
    &elm::LEXER,
    &emacslisp::LEXER,
    &erlang::LEXER,
    &factor::LEXER,
    &fennel::LEXER,
    &fish::LEXER,
    &forth::LEXER,
    &fortran::LEXER,
    &fortranfixed::LEXER,
    &fsharp::LEXER,
    &gas::LEXER,
    &gdscript::LEXER,
    &gdscript3::LEXER,
    &gherkin::LEXER,
    &gleam::LEXER,
    &glsl::LEXER,
    &gnuplot::LEXER,
    &go_template::LEXER,
    &graphql::LEXER,
    &groff::LEXER,
    &groovy::LEXER,
    &handlebars::LEXER,
    &hare::LEXER,
    &haskell::LEXER,
    &hcl::LEXER,
    &hexdump::LEXER,
    &hlb::LEXER,
    &hlsl::LEXER,
    &holyc::LEXER,
    &html::LEXER,
    &hy::LEXER,
    &idris::LEXER,
    &igor::LEXER,
    &ini::LEXER,
    &io::LEXER,
    &iscdhcpd::LEXER,
    &j::LEXER,
    &janet::LEXER,
    &java::LEXER,
    &javascript::LEXER,
    &json::LEXER,
    &jsonata::LEXER,
    &jsonnet::LEXER,
    &julia::LEXER,
    &jungle::LEXER,
    &kotlin::LEXER,
    &lean::LEXER,
    &lighttpd_configuration_file::LEXER,
    &llvm::LEXER,
    &lua::LEXER,
    &makefile::LEXER,
    &mako::LEXER,
    &mason::LEXER,
    &materialize_sql_dialect::LEXER,
    &mathematica::LEXER,
    &matlab::LEXER,
    &mcfunction::LEXER,
    &meson::LEXER,
    &metal::LEXER,
    &minizinc::LEXER,
    &mlir::LEXER,
    &modula_2::LEXER,
    &mojo::LEXER,
    &monkeyc::LEXER,
    &moonscript::LEXER,
    &morrowindscript::LEXER,
    &myghty::LEXER,
    &mysql::LEXER,
    &nasm::LEXER,
    &natural::LEXER,
    &ndisasm::LEXER,
    &newspeak::LEXER,
    &nginx_configuration_file::LEXER,
    &nim::LEXER,
    &nix::LEXER,
    &nsis::LEXER,
    &objective_c::LEXER,
    &objectpascal::LEXER,
    &ocaml::LEXER,
    &octave::LEXER,
    &odin::LEXER,
    &onesenterprise::LEXER,
    &openedge_abl::LEXER,
    &openscad::LEXER,
    &org_mode::LEXER,
    &pacmanconf::LEXER,
    &perl::LEXER,
    &php::LEXER,
    &pig::LEXER,
    &pkgconfig::LEXER,
    &pl_pgsql::LEXER,
    &plaintext::LEXER,
    &plutus_core::LEXER,
    &pony::LEXER,
    &postgresql_sql_dialect::LEXER,
    &postscript::LEXER,
    &povray::LEXER,
    &powerquery::LEXER,
    &powershell::LEXER,
    &prolog::LEXER,
    &promela::LEXER,
    &promql::LEXER,
    &properties::LEXER,
    &protocol_buffer::LEXER,
    &prql::LEXER,
    &psl::LEXER,
    &puppet::LEXER,
    &python::LEXER,
    &python_2::LEXER,
    &qbasic::LEXER,
    &qml::LEXER,
    &r::LEXER,
    &racket::LEXER,
    &ragel::LEXER,
    &react::LEXER,
    &reasonml::LEXER,
    &reg::LEXER,
    &rego::LEXER,
    &rexx::LEXER,
    &rpgle::LEXER,
    &rpm_spec::LEXER,
    &ruby::LEXER,
    &rust::LEXER,
    &sas::LEXER,
    &sass::LEXER,
    &scala::LEXER,
    &scheme::LEXER,
    &scilab::LEXER,
    &scss::LEXER,
    &sed::LEXER,
    &sieve::LEXER,
    &smali::LEXER,
    &smalltalk::LEXER,
    &smarty::LEXER,
    &snbt::LEXER,
    &snobol::LEXER,
    &solidity::LEXER,
    &sourcepawn::LEXER,
    &sparql::LEXER,
    &sql::LEXER,
    &squidconf::LEXER,
    &standard_ml::LEXER,
    &stas::LEXER,
    &stylus::LEXER,
    &swift::LEXER,
    &systemd::LEXER,
    &systemverilog::LEXER,
    &tablegen::LEXER,
    &tal::LEXER,
    &tasm::LEXER,
    &tcl::LEXER,
    &tcsh::LEXER,
    &termcap::LEXER,
    &terminfo::LEXER,
    &terraform::LEXER,
    &tex::LEXER,
    &thrift::LEXER,
    &toml::LEXER,
    &tradingview::LEXER,
    &transact_sql::LEXER,
    &turing::LEXER,
    &turtle::LEXER,
    &twig::LEXER,
    &typescript::LEXER,
    &typoscript::LEXER,
    &typoscriptcssdata::LEXER,
    &typoscripthtmldata::LEXER,
    &typst::LEXER,
    &ucode::LEXER,
    &v::LEXER,
    &v_shell::LEXER,
    &vala::LEXER,
    &vb_net::LEXER,
    &verilog::LEXER,
    &vhdl::LEXER,
    &vhs::LEXER,
    &viml::LEXER,
    &vue::LEXER,
    &wdte::LEXER,
    &webgpu_shading_language::LEXER,
    &webvtt::LEXER,
    &whiley::LEXER,
    &xml::LEXER,
    &xorg::LEXER,
    &yaml::LEXER,
    &yang::LEXER,
    &z80_assembly::LEXER,
    &zed::LEXER,
    &zig::LEXER,
];
