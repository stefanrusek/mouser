import assert from 'node:assert/strict';
import test from 'node:test';
import { Window } from 'happy-dom';
import { renderMarkdown } from './markdown.js';

const html = (markdown) => {
  const doc = new Window().document;
  const root = doc.createElement('div');
  root.append(renderMarkdown(doc, markdown));
  return root.innerHTML;
};

test('headings become heading elements of their level', () => {
  assert.equal(html('# One\n\n### Three'), '<h1>One</h1><h3>Three</h3>');
});

test('lines of text are one paragraph and a blank line starts the next', () => {
  assert.equal(html('first\nsecond\n\nthird'), '<p>first second</p><p>third</p>');
});

test('dash and star items make a list and numbered items an ordered list', () => {
  assert.equal(html('- a\n* b\n\n1. c\n2. d'), '<ul><li>a</li><li>b</li></ul><ol><li>c</li><li>d</li></ol>');
});

test('fenced code is a block, even when the fence is indented, and keeps its line breaks', () => {
  assert.equal(html('text\n\n   ```\n   mouser status\n   mouser logs\n   ```\nafter'), '<p>text</p><pre><code>   mouser status\n   mouser logs</code></pre><p>after</p>');
});

test('backticks make inline code', () => {
  assert.equal(html('run `mouser status` now'), '<p>run <code>mouser status</code> now</p>');
  assert.equal(html('- the `port` key'), '<ul><li>the <code>port</code> key</li></ul>');
});

test('HTML in the text is shown as text, never run', () => {
  assert.equal(html('<script>x()</script> & `<b>`'), '<p>&lt;script&gt;x()&lt;/script&gt; &amp; <code>&lt;b&gt;</code></p>');
  assert.equal(html('```\n<i>\n```'), '<pre><code>&lt;i&gt;</code></pre>');
});

test('an empty document renders nothing', () => {
  assert.equal(html(''), '');
});
