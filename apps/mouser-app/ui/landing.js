// The landing highlight: a ring shown briefly where the pointer arrived.

export const HIGHLIGHT_MS = 700;

export async function init({ doc, api, after }) {
  const ring = doc.getElementById('highlight');
  await api.onLanding(({ x, y }) => {
    ring.style.left = `${x}px`;
    ring.style.top = `${y}px`;
    ring.hidden = false;
    after(HIGHLIGHT_MS, async () => {
      ring.hidden = true;
      await api.hideWindow();
    });
  });
}
