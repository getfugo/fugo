// {{ printf "%s" .api }}
const endpoint = "{{ .api }}/comments";
{{- range $i, $e := .list }}
const item{{ $i }} = "{{ $e }}";
{{- end }}
{{ with .site }}const title = "{{ .title }}";{{ else }}const title = "none";{{ end }}
{{ if eq .mode "prod" }}const prod = true;{{ else }}const prod = false;{{ end }}
