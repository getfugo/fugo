export default function helper(n) {
  let sum = 0;
  for (const x of [1, 2, 3]) sum += x * n;
  return sum;
}
