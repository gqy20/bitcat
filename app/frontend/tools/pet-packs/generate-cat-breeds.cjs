const { execFileSync } = require('node:child_process');
const path = require('node:path');

const { CAT_BREEDS, DEFAULT_BREED_ID } = require('./cat-breeds.cjs');

const generatorPath = path.join(__dirname, '..', 'generate-cat-pack.cjs');

function parseBreedIds(argv) {
  const explicit = [];
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--all') return Object.keys(CAT_BREEDS);
    if (arg === '--breed') {
      explicit.push(argv[i + 1]);
      i += 1;
    } else if (arg.startsWith('--breed=')) {
      explicit.push(arg.slice('--breed='.length));
    }
  }
  return explicit.filter(Boolean).length ? explicit.filter(Boolean) : [DEFAULT_BREED_ID];
}

function generateBreed(id) {
  execFileSync(process.execPath, [generatorPath, '--breed', id], { stdio: 'inherit' });
}

for (const id of parseBreedIds(process.argv.slice(2))) {
  generateBreed(id);
}
