import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const frontendRoot = path.resolve(__dirname, '..');
const generator = path.join(__dirname, 'generate_arena_glb.py');
const outDir = path.join(frontendRoot, 'assets', 'arena');

function candidateBlenderPaths() {
  const envPath = process.env.BLENDER_BIN || process.env.BLENDER_PATH;
  const paths = [];
  if (envPath) paths.push(envPath);
  paths.push('blender');
  const programFiles = process.env.ProgramFiles || 'C:\\Program Files';
  const programFilesX86 = process.env['ProgramFiles(x86)'] || 'C:\\Program Files (x86)';
  for (const base of [programFiles, programFilesX86]) {
    paths.push(path.join(base, 'Blender Foundation', 'Blender 4.3', 'blender.exe'));
    paths.push(path.join(base, 'Blender Foundation', 'Blender 4.2', 'blender.exe'));
    paths.push(path.join(base, 'Blender Foundation', 'Blender 4.1', 'blender.exe'));
    paths.push(path.join(base, 'Blender Foundation', 'Blender 4.0', 'blender.exe'));
    paths.push(path.join(base, 'Blender Foundation', 'Blender 3.6', 'blender.exe'));
  }
  paths.push('D:\\tools\\Blender\\Blender_5.1\\blender.exe');
  paths.push('D:\\tools\\Blender\\blender.exe');
  return paths;
}

function findBlender() {
  for (const candidate of candidateBlenderPaths()) {
    if (candidate === 'blender') {
      const probe = spawnSync(candidate, ['--version'], { shell: true, encoding: 'utf8' });
      if (!probe.error && probe.status === 0) return candidate;
      continue;
    }
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

const blender = findBlender();
if (!blender) {
  console.error('Blender was not found. Set BLENDER_BIN to blender.exe and retry.');
  process.exit(1);
}

const result = spawnSync(blender, [
  '--background',
  '--python',
  generator,
  '--',
  '--out-dir',
  outDir,
  '--variant',
  'all',
], {
  cwd: frontendRoot,
  stdio: 'inherit',
  shell: blender === 'blender',
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}
for (const file of ['player.glb', 'enemy.glb']) {
  const target = path.join(outDir, file);
  if (!existsSync(target)) {
    console.error(`Expected output was not created: ${target}`);
    process.exit(1);
  }
}
process.exit(result.status ?? 0);
