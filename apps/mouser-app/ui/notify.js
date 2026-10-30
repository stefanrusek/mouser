// Problem banners: keyed by (problem_id, computer), each carrying the name of
// the computer it concerns. They appear only in Health, and as counts in Desk.

import { el } from './dom.js';
import { renderFix } from './fixes.js';

export const problemKey = (problem) => JSON.stringify([problem.problem_id, problem.computer]);

// `problems` with each (problem_id, computer) once, the first one kept.
export function uniqueProblems(problems) {
  const seen = new Set();
  return problems.filter((problem) => {
    const key = problemKey(problem);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

// The banner for `problem` of the computer called `computerName`, with its fix as one action.
export function renderBanner(doc, api, problem, computerName, navigate) {
  const banner = el(
    doc,
    'div',
    { class: 'problem', role: 'alert', 'data-computer': problem.computer },
    el(doc, 'small', { class: 'where' }, computerName),
    el(doc, 'p', { class: 'message' }, problem.message),
  );
  const fix = renderFix(doc, api, problem, navigate, computerName);
  if (fix) banner.append(fix);
  return banner;
}
