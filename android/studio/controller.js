// KONKR-only controller adapter for the unchanged original Cart Studio.
// Keep touch, keyboard and document file picker behavior intact.
(() => {
  const visible = (el) =>
    el && !el.disabled && !el.closest('[hidden]') &&
    !!(el.offsetWidth || el.offsetHeight || el.getClientRects().length);
  const editor = () => document.getElementById('editor');
  const activeArea = () =>
    editor() && !editor().hidden ? editor() :
    document.querySelector('dialog[open]') || document.getElementById('grid');
  const controls = () => {
    const root = activeArea();
    if (!root) return [];
    const selectors = 'button, input:not([type="file"]), [role="button"], a[href], [tabindex="0"]';
    const elements = [...root.querySelectorAll(selectors)].filter(visible);
    if (root.id === 'grid') {
      elements.unshift(...[...document.querySelectorAll('#tabs button')].filter(visible));
      const save = document.querySelector('#write');
      if (visible(save)) elements.push(save);
    }
    return elements;
  };
  const setFocus = (el) => {
    if (!el) return;
    el.focus({ preventScroll: true });
    el.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'instant' });
  };
  const cycle = (direction) => {
    const tabs = [...document.querySelectorAll('#tabs button')].filter(visible);
    if (tabs.length < 2) return;
    let current = tabs.findIndex(el => el.getAttribute('aria-pressed') === 'true');
    if (current < 0) current = 0;
    const next = tabs[(current + direction + tabs.length) % tabs.length];
    next.click();
    setFocus(next);
  };
  const navigate = (dir) => {
    const elements = controls();
    if (!elements.length) return;
    let current = document.activeElement;
    if (!elements.includes(current)) return setFocus(elements[0]);
    const rect = current.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    const scored = elements.filter(el => el !== current).map(el => {
      const b = el.getBoundingClientRect();
      const dx = b.left + b.width / 2 - x;
      const dy = b.top + b.height / 2 - y;
      const ahead = (dir === 'left' && dx < -4) || (dir === 'right' && dx > 4) ||
        (dir === 'up' && dy < -4) || (dir === 'down' && dy > 4);
      const major = (dir === 'left' || dir === 'right') ? Math.abs(dx) : Math.abs(dy);
      const minor = (dir === 'left' || dir === 'right') ? Math.abs(dy) : Math.abs(dx);
      return { el, score: major + minor * 2, ahead };
    }).filter(x => x.ahead).sort((a, b) => a.score - b.score);
    if (scored.length) setFocus(scored[0].el);
    else {
      // Scroll through very long libraries without losing the input focus.
      window.scrollBy(0, dir === 'down' ? 220 : dir === 'up' ? -220 : 0);
    }
  };
  window.SlotController = {
    key(code) {
      switch (code) {
        case 'L1': cycle(-1); break;
        case 'R1': cycle(1); break;
        case 'A': {
          const focused = document.activeElement;
          if (visible(focused) && focused.matches('input[type="search"],input[type="color"]'))
            return;
          if (visible(focused)) focused.click();
          else setFocus(controls()[0]);
          break;
        }
        case 'B': {
          const pop = document.querySelector('dialog[open]');
          if (pop) {
            pop.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
            if (pop.close) pop.close();
          } else if (!editor()?.hidden) {
            document.getElementById('ed-close')?.click();
          } else {
            window.AndroidNav?.back?.();
          }
          break;
        }
        case 'UP': case 'DOWN': case 'LEFT': case 'RIGHT':
          // Preserve native text navigation while editing a search query.
          if (document.activeElement?.matches('input[type="search"]')) return;
          navigate(code.toLowerCase()); break;
        case 'Y': document.getElementById('write')?.click(); break;
      }
    },
  };
})();
