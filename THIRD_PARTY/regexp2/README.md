# regexp2: the .NET regular-expression dialect

`crates/highlight/src/regexp2/` ports [regexp2](https://github.com/dlclark/regexp2) v1.11.5
(MIT, `LICENSE`), the engine Chroma compiles its lexer rules with: its parser and character
classes (`syntax/parser.go`, `syntax/charclass.go`, `syntax/tree.go`, with the case-mapping
table) and its matcher's backtracking rules (`runner.go`, `syntax/writer.go`). regexp2 is itself
a port of .NET's `System.Text.RegularExpressions` (dotnet/corefx, MIT); `ATTRIB` is regexp2's
attribution file with that licence.
