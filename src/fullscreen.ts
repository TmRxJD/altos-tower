export function setupFullscreen() {
  const buttons = [...document.querySelectorAll<HTMLButtonElement>('[data-fullscreen]')];
  const status = document.createElement('span');
  status.className = 'fullscreen-status';
  status.setAttribute('role', 'status');
  document.body.append(status);
  const update = () => buttons.forEach(button => {
    const active = !!document.fullscreenElement || document.documentElement.classList.contains('viewport-play');
    button.setAttribute('aria-pressed', String(active));
    button.setAttribute('aria-label', active ? 'Exit fullscreen' : 'Enter fullscreen');
    button.title = active ? 'Exit fullscreen' : 'Enter fullscreen';
  });
  buttons.forEach(button => button.addEventListener('click', async () => {
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else if (document.documentElement.classList.contains('viewport-play')) document.documentElement.classList.remove('viewport-play');
      else if (document.fullscreenEnabled && document.documentElement.requestFullscreen) await document.documentElement.requestFullscreen();
      else throw new Error('Native fullscreen unavailable');
      status.textContent = '';
    } catch {
      document.documentElement.classList.add('viewport-play');
      status.textContent = 'Playing in the full browser viewport. Hide browser controls for more space.';
      window.setTimeout(() => { status.textContent = ''; }, 5000);
    }
    update();
  }));
  document.addEventListener('fullscreenchange', update);
  update();
}
