import { Mode, area } from './mod';
enum Color { Red, Green = 'g' }
class Box<T> {
  constructor(private readonly v: T) {}
  get value(): T { return this.v; }
}
const b = new Box<number>(area(3));
console.log(Color.Green, Mode.A, b.value);
