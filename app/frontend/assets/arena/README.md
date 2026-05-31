# Arena Model Assets

Optional GLTF/GLB assets for BitCat Arena live here. The game falls back to its
procedural low-poly fighters when these files are missing.

Recommended first-pass files:

- `player.glb`
- `enemy.glb`

Animation clips are matched by lowercase keywords in clip names:

- idle: `idle`
- run: `run`, `walk`
- jump: `jump`
- light attack: `light`, `punch`, `attack`
- heavy attack: `heavy`, `kick`
- guard: `guard`, `block`
- hurt: `hurt`, `hit`
- dead: `dead`, `ko`
- win: `win`, `victory`

Keep model scale close to a 2-unit tall character. The loader normalizes common
sizes, but small, centered humanoids are easier to tune with the hitboxes.
