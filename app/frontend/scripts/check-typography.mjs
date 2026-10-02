// Prevent new local typography numbers in production window styles.
// Canvas game text and @font-face weight ranges are intentionally outside this check.
import { readFile, readdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const files = (await readdir(path.join(root, 'css')))
  .filter(name => name.endsWith('.css') && !['fonts.css', 'typography.css'].includes(name))
  .map(name => `css/${name}`);
files.push('voice.html');
const definitions = await readFile(path.join(root, 'css/typography.css'), 'utf8');
const roles = new Set([...definitions.matchAll(/(--[\w-]+)\s*:/g)].map(match => match[1]));
const violations = [];
for (const file of files) {
  const content = (await readFile(path.join(root, file), 'utf8')).replace(/\/\*[\s\S]*?\*\//g, comment => comment.replace(/[^\n]/g, ' '));
  for (const [index, line] of content.split('\n').entries()) {
    for (const match of line.matchAll(/(--[\w-]+)\s*:/g)) {
      if (roles.has(match[1])) violations.push(`${file}:${index + 1}: define ${match[1]} only in typography.css`);
    }
    for (const match of line.matchAll(/var\((--(?:type-|weight-|leading-)[\w-]+)/g)) {
      if (!roles.has(match[1])) violations.push(`${file}:${index + 1}: unknown typography role ${match[1]}`);
    }
    if (/(?:font-size|font-weight|line-height)\s*:\s*(?:[\d.]|bold\b|normal\b)|\bfont\s*:\s*\d/.test(line)) {
      violations.push(`${file}:${index + 1}: use a typography role instead of a local numeric value`);
    }
  }
}
if (violations.length) {
  console.error(violations.join('\n'));
  process.exitCode = 1;
} else {
  console.log(`Typography roles verified in ${files.length} production style files.`);
}
