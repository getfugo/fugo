import { greet, VERSION } from './lib/util';
import helper from 'js/lib/helper';
import * as params from '@params';
import data from './data.json';

const out = [greet('world'), helper(2), VERSION, params.api, data.items.length];
export default out;
console.log(out, `x=${params.n ?? 0}`);
