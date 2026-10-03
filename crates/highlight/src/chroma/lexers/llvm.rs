//! Chroma's `llvm.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "llvm",
    config: ConfigDef {
        name: "LLVM",
        aliases: &["llvm"],
        filenames: &["*.ll"],
        mime_types: &["text/x-llvm"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            include("whitespace"),
            rule(r#"([-a-zA-Z$._][\w\-$.]*|"[^"]*?")\s*:"#).token(T::NameLabel),
            include("keyword"),
            rule(r#"%([-a-zA-Z$._][\w\-$.]*|"[^"]*?")"#).token(T::NameVariable),
            rule(r#"@([-a-zA-Z$._][\w\-$.]*|"[^"]*?")"#).token(T::NameVariableGlobal),
            rule(r"%\d+").token(T::NameVariableAnonymous),
            rule(r"@\d+").token(T::NameVariableGlobal),
            rule(r"#\d+").token(T::NameVariableGlobal),
            rule(r#"!([-a-zA-Z$._][\w\-$.]*|"[^"]*?")"#).token(T::NameVariable),
            rule(r"!\d+").token(T::NameVariableAnonymous),
            rule(r#"c?"[^"]*?""#).token(T::LiteralString),
            rule(r"0[xX][a-fA-F0-9]+").token(T::LiteralNumber),
            rule(r"-?\d+(?:[.]\d+)?(?:[eE][-+]?\d+(?:[.]\d+)?)?").token(T::LiteralNumber),
            rule(r"[=<>{}\[\]()*.,!]|x\b").token(T::Punctuation),
        ]),
        ("whitespace", &[
            rule(r"(\n|\s)+").token(T::Text),
            rule(r";.*?\n").token(T::Comment),
        ]),
        ("keyword", &[
            rule(r"(inaccessiblemem_or_argmemonly|dereferenceable_or_null|externally_initialized|available_externally|linker_private_weak|inaccessiblememonly|local_unnamed_addr|sanitize_address|x86_vectorcallcc|noimplicitfloat|sanitize_thread|source_filename|sanitize_memory|preserve_mostcc|dereferenceable|arm_aapcs_vfpcc|zeroinitializer|extractelement|linker_private|cxx_fast_tlscc|x86_64_win64cc|intel_ocl_bicc|x86_fastcallcc|preserve_allcc|x86_thiscallcc|x86_stdcallcc|amdgpu_kernel|x86_64_sysvcc|addrspacecast|getelementptr|returns_twice|insertelement|shufflevector|msp430_intrcc|extractvalue|linkonce_odr|inteldialect|alwaysinline|blockaddress|noduplicates|thread_local|singlethread|localdynamic|unnamed_addr|avr_signalcc|unreachable|personality|arm_aapcscc|webkit_jscc|catchswitch|initialexec|insertvalue|noduplicate|nonlazybind|extern_weak|spir_kernel|attributes|sideeffect|cleanuppad|cleanupret|inlinehint|convergent|argmemonly|exactmatch|datalayout|landingpad|arm_apcscc|indirectbr|swifterror|ptx_device|ptx_kernel|x86_intrcc|avr_intrcc|alignstack|norecurse|amdgpu_vs|writeonly|localexec|amdgpu_ps|monotonic|amdgpu_gs|amdgpu_cs|allocsize|atomicrmw|swiftself|unordered|sspstrong|nocapture|addrspace|getresult|safestack|appending|noredzone|dllimport|dllexport|nobuiltin|jumptable|protected|spir_func|inbounds|musttail|external|volatile|inalloca|prologue|hhvm_ccc|noinline|readonly|readnone|linkonce|internal|catchret|samesize|returned|ptrtoint|inttoptr|noreturn|constant|nounwind|catchpad|anyregcc|weak_odr|distinct|release|optnone|uwtable|declare|optsize|swiftcc|noalias|private|minsize|default|fptrunc|zeroext|bitcast|nonnull|acq_rel|acquire|signext|builtin|cmpxchg|cleanup|section|largest|deplibs|seq_cst|common|hhvmcc|fptosi|opaque|triple|fptoui|target|filter|comdat|sitofp|module|switch|prefix|fastcc|coldcc|atomic|va_arg|notail|hidden|caller|global|invoke|define|sspreq|within|unwind|alloca|malloc|resume|select|uitofp|store|trunc|naked|byval|fence|fpext|begin|alias|exact|ghccc|ifunc|catch|undef|false|inreg|align|xchg|none|type|sext|call|null|cold|fcmp|icmp|tail|srem|free|load|nand|ashr|lshr|nnan|from|nest|frem|true|sret|urem|fdiv|zext|sdiv|udiv|ninf|arcp|fmul|fast|umax|fsub|umin|weak|fadd|ret|oeq|asm|add|dbg|sub|ccc|one|mul|olt|any|uge|nsz|ule|shl|ogt|ssp|and|sgt|nsw|nuw|min|max|sle|xor|sge|ult|phi|end|ole|une|ueq|uno|ord|oge|slt|ugt|eq|ne|to|gc|cc|or|br|c|x)\b").token(T::Keyword),
            rule(r"(ppc_fp128|metadata|x86_fp80|double|label|fp128|float|token|void|half)").token(T::KeywordType),
            rule(r"i[1-9]\d*").token(T::Keyword),
        ]),
    ],
};
