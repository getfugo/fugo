export function Comp(props: { name: string }) {
  return <><h1 class="t">Hi {props.name}</h1><p /></>;
}
console.log(Comp({ name: 'x' }));
