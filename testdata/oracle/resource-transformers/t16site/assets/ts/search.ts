const Mustache = require('mustache-lite');
const api = 'https://api.example.org';
export async function search(q: string): Promise<string> {
  const r = await fetch(`${api}/search?q=${encodeURIComponent(q)}`);
  const j: { hits: { title: string }[] } = await r.json();
  return Mustache.render('{{#hits}}<li>{{title}}</li>{{/hits}}', j);
}
(window as any).search = search;
