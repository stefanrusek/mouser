// A small Markdown renderer for the shipped troubleshooting guide: headings,
// paragraphs, lists and code. Everything is set as text, so HTML is escaped.

import { el } from './dom.js';

const HEADING = /^(#{1,6})\s+(.*)$/;
const ITEM = /^\s*([-*]|\d+\.)\s+(.*)$/;

// Text with `code` spans as nodes.
function inline(doc, text) {
  return text.split('`').map((part, index) => (index % 2 === 1 ? el(doc, 'code', {}, part) : part));
}

export function renderMarkdown(doc, markdown) {
  const out = doc.createDocumentFragment();
  let paragraph = [];
  let list = null;
  const endParagraph = () => {
    if (paragraph.length > 0) out.append(el(doc, 'p', {}, ...inline(doc, paragraph.join(' '))));
    paragraph = [];
  };
  const end = () => {
    endParagraph();
    list = null;
  };
  const lines = markdown.split('\n');
  for (let at = 0; at < lines.length; at += 1) {
    const line = lines[at];
    if (line.trim().startsWith('```')) {
      end();
      const code = [];
      for (at += 1; !lines[at].trim().startsWith('```'); at += 1) code.push(lines[at]);
      out.append(el(doc, 'pre', {}, el(doc, 'code', {}, code.join('\n'))));
    } else if (line.trim() === '') {
      end();
    } else if (HEADING.test(line)) {
      end();
      const [, marks, text] = HEADING.exec(line);
      out.append(el(doc, `h${marks.length}`, {}, ...inline(doc, text)));
    } else if (ITEM.test(line)) {
      const [, marker, text] = ITEM.exec(line);
      endParagraph();
      if (!list) {
        list = el(doc, /\d/.test(marker) ? 'ol' : 'ul');
        out.append(list);
      }
      list.append(el(doc, 'li', {}, ...inline(doc, text)));
    } else {
      list = null;
      paragraph.push(line.trim());
    }
  }
  end();
  return out;
}
