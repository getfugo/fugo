const api: string = "{{ .api }}";
export function search(q: string): string {
  return api + "/search?q=" + q;
}
