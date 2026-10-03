## Security Policy

### Reporting a Vulnerability

Please report (suspected) security vulnerabilities in fugo privately through GitHub's
[private vulnerability reporting](https://github.com/getfugo/fugo/security/advisories/new)
for this repository, not in a public issue or discussion. If we can confirm the issue, we will
release a patch as soon as possible depending on the complexity of the issue.

Report only fugo issues here.

### Security model

Templates and themes are code. fugo's `security` configuration (`exec.allow`, `exec.osEnv`,
`funcs.getenv`, `http.urls`, `http.methods`;
[docs/content/configuration/security.md](docs/content/configuration/security.md)) limits the
programs they may run, the environment variables they may read and the URLs they may fetch.
Layouts are Tera templates, not Go's `html/template`: output is escaped by output format (HTML
and XML), not by context, so a value in `<script>` needs `jsonify | safe` and one in a query
string `urlencode` ([docs/rust-port/template-api.md](docs/rust-port/template-api.md)).
