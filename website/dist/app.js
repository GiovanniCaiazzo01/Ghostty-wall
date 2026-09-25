const menuButton = document.querySelector('.mobile-menu');
const sidebar = document.querySelector('.sidebar');
const backdrop = document.querySelector('.drawer-backdrop');
function setMenu(open) {
  sidebar.classList.toggle('open', open);
  backdrop.classList.toggle('visible', open);
  document.body.classList.toggle('menu-open', open);
  menuButton.setAttribute('aria-expanded', String(open));
  menuButton.setAttribute('aria-label', open ? 'Close navigation' : 'Open navigation');
  if (open) sidebar.querySelector('a').focus();
}
menuButton.addEventListener('click', () => setMenu(!sidebar.classList.contains('open')));
backdrop.addEventListener('click', () => {setMenu(false); menuButton.focus();});
document.addEventListener('keydown', event => {
  if (!sidebar.classList.contains('open')) return;
  if (event.key === 'Escape') {setMenu(false); menuButton.focus();}
  if (event.key === 'Tab') {
    const links = [...sidebar.querySelectorAll('a'), menuButton];
    const current = links.indexOf(document.activeElement);
    const next = event.shiftKey ? (current - 1 + links.length) % links.length : (current + 1) % links.length;
    event.preventDefault(); links[next].focus();
  }
});
window.matchMedia('(min-width: 761px)').addEventListener('change', e => {if(e.matches) setMenu(false);});
let statusTimer;
const status = document.getElementById('copy-status');
function announce(message) {
  clearTimeout(statusTimer);
  status.textContent = message;
  status.classList.add('visible');
  statusTimer = setTimeout(() => {status.classList.remove('visible');}, 2400);
}
async function copyText(text) {
  if(navigator.clipboard && window.isSecureContext) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const area = document.createElement('textarea');
  area.value = text;
  area.style.position = 'fixed';
  area.style.opacity = '0';
  document.body.appendChild(area);
  area.select();
  const copied = document.execCommand('copy');
  area.remove();
  if(!copied) throw new Error('Copy unavailable');
}
document.querySelectorAll('.copy-button').forEach(button => {
  button.addEventListener('click', async () => {
    const code = button.closest('.code-block').querySelector('code');
    const original = button.innerHTML;
    try {
      await copyText(code.textContent);
      button.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" aria-hidden="true"><path d="m5 12 4 4L19 6"/></svg><span>Copied</span>';
      button.classList.add('copied');
      button.disabled = true;
      announce('Code copied to clipboard');
      setTimeout(() => {button.innerHTML = original; button.classList.remove('copied'); button.disabled = false;}, 2000);
    } catch {
      const range = document.createRange();
      range.selectNodeContents(code);
      const selection = window.getSelection();
      selection.removeAllRanges(); selection.addRange(range);
      announce('Code selected. Press Ctrl+C or ⌘C to copy.');
    }
  });
});
const tocLinks = [...document.querySelectorAll('.toc nav a')];
const sections = tocLinks.map(link => document.getElementById(link.hash.slice(1))).filter(Boolean);
function updateToc() {
  let active = sections[0];
  for(const section of sections) if(section.getBoundingClientRect().top <= 160) active = section;
  tocLinks.forEach(link => {
    const selected = active && link.hash === '#' + active.id;
    link.classList.toggle('active', selected);
    if(selected) link.setAttribute('aria-current','location'); else link.removeAttribute('aria-current');
  });
}
let ticking = false;
window.addEventListener('scroll', () => {
  if(!ticking) requestAnimationFrame(() => {updateToc(); ticking = false;});
  ticking = true;
}, {passive:true});
updateToc();
