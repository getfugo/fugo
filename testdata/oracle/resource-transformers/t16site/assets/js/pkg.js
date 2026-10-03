import fake, { named } from 'fakepkg';
import sub from 'fakepkg/sub';
const cjs = require('cjspkg');
import inner from 'innerpkg';
console.log(fake, named, sub, cjs.value, inner);
