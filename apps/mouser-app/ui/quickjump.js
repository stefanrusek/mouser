// QuickJump: one large letter centred on a display; ending QuickJump removes it.

export async function init({ doc, api }) {
  const letter = doc.getElementById('letter');
  letter.textContent = new URLSearchParams(doc.location.search).get('letter');
  letter.hidden = false;
  await api.onQuickJump(() => {
    letter.textContent = '';
    letter.hidden = true;
  });
}
