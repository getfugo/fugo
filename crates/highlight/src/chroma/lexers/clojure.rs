//! Chroma's `clojure.xml` lexer, converted to Rust (crate README, "Lexer and style files").

use crate::chroma::defs::prelude::*;

#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "clojure",
    config: ConfigDef {
        name: "Clojure",
        aliases: &["clojure", "clj", "edn"],
        filenames: &["*.clj", "*.edn"],
        mime_types: &["text/x-clojure", "application/x-clojure", "application/edn"],
        ..ConfigDef::EMPTY
    },
    states: &[
        ("root", &[
            rule(r";.*$").token(T::CommentSingle),
            rule(r"[,\s]+").token(T::Text),
            rule(r"-?\d+\.\d+").token(T::LiteralNumberFloat),
            rule(r"-?\d+").token(T::LiteralNumberInteger),
            rule(r"0x-?[abcdef\d]+").token(T::LiteralNumberHex),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            rule(r"'(?!#)[\w!$%*+<=>?/.#-]+").token(T::LiteralStringSymbol),
            rule(r"\\(.|[a-z]+)").token(T::LiteralStringChar),
            rule(r"::?#?(?!#)[\w!$%*+<=>?/.#-]+").token(T::LiteralStringSymbol),
            rule(r"~@|[`\'#^~&@]").token(T::Operator),
            rule(r"(quote|loop|new|var|let|def|if|do|fn|\.) ").token(T::Keyword),
            rule(r"(definterface|defprotocol|defproject|defstruct|definline|defmethod|defrecord|defmulti|defmacro|defonce|declare|deftype|defn-|def-|defn|ns) ").token(T::KeywordDeclaration),
            rule(r"(clear-agent-errors|construct-proxy|bit-shift-right|get-proxy-class|special-symbol\?|with-local-vars|proxy-mappings|bit-shift-left|sorted-map-by|macroexpand-1|remove-method|create-struct|resultset-seq|inspect-table|inspect-tree|update-proxy|aset-boolean|agent-errors|with-out-str|insert-child|append-child|intersection|insert-right|to-array-2d|rename-keys|println-str|macroexpand|aset-double|select-keys|insert-left|aset-float|aset-short|interleave|re-pattern|make-array|identical\?|take-while|into-array|re-matches|re-matcher|complement|vector-zip|drop-while|when-first|map-invert|sorted-map|ns-resolve|difference|sorted-set|merge-with|ns-publics|split-with|ns-interns|ns-imports|constantly|struct-map|comparator|not-every\?|aset-long|print-str|re-groups|lazy-cons|remove-ns|namespace|await-for|contains\?|array-map|create-ns|make-node|with-meta|with-open|instance\?|ns-refers|aset-byte|aset-char|load-file|read-line|replicate|send-off|aset-int|distinct|not-any\?|take-nth|tree-seq|split-at|to-array|ns-unmap|identity|find-doc|find-var|hash-set|when-not|children|when-let|lazy-cat|hash-map|line-seq|rand-int|keyword\?|file-seq|accessor|replace|bit-not|find-ns|resolve|bit-and|println|binding|locking|vector\?|partial|nthrest|max-key|bit-xor|dotimes|ref-set|xml-seq|boolean|var-get|seq-zip|sort-by|branch\?|butlast|symbol\?|project|min-key|ns-name|comment|string\?|iterate|commute|alength|xml-zip|keyword|newline|re-find|reverse|var-set|prn-str|bit-or|import|re-seq|rights|assert|reduce|remove|gensym|rename|filter|ffirst|if-let|false\?|pr-str|every\?|vector|mapcat|ensure|rfirst|concat|second|double|select|dosync|symbol|subvec|if-not|ns-map|struct|zipper|zipmap|all-ns|dissoc|repeat|assoc|cycle|class|deref|zero\?|slurp|short|dorun|doseq|merge|memfn|agent|rrest|count|parse|right|float|flush|alter|fnseq|frest|doall|print|refer|in-ns|apply|union|await|list\*|proxy|lefts|true\?|index|first|range|left|keys|aset|join|into|last|read|rand|list|load|long|loop|conj|test|vals|pos\?|bean|peek|subs|path|time|find|rest|eval|end\?|edit|map\?|drop|root|aget|rseq|down|doto|meta|send|when|byte|take|seq\?|sync|name|neg\?|some|sort|cast|char|disj|next|not=|nil\?|node|comp|cond|cons|quot|var\?|max|new|rem|set|doc|seq|for|get|ref|inc|int|key|not|prn|min|map|val|nth|dec|pop|and|str|pr|\.\.|up|>=|->|==|<=|or|=|/|>|-|\*|\+|<) ").token(T::NameBuiltin),
            rule(r"(?<=\()(?!#)[\w!$%*+<=>?/.#-]+").token(T::NameFunction),
            rule(r"(?!#)[\w!$%*+<=>?/.#-]+").token(T::NameVariable),
            rule(r"(\[|\])").token(T::Punctuation),
            rule(r"(\{|\})").token(T::Punctuation),
            rule(r"(\(|\))").token(T::Punctuation),
        ]),
    ],
};
