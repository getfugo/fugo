const KEY = 'theme';
function apply(t: string): void {
  document.documentElement.setAttribute('data-theme', t);
  localStorage.setItem(KEY, t);
}
document.querySelectorAll<HTMLElement>('[data-theme-set]').forEach((el) => {
  el.addEventListener('click', () => apply(el.dataset.themeSet ?? 'light'));
});
