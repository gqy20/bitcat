import argparse
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from arena_glb.export import build_variant
from arena_glb.palettes import VARIANTS


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--variant", default="all", choices=["all", "player", "enemy"])
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else sys.argv[1:]
    args = parser.parse_args(argv)

    for name, palette in VARIANTS.items():
        if args.variant != "all" and args.variant != name:
            continue
        build_variant(args.out_dir, f"{name}.glb", palette)


if __name__ == "__main__":
    main()
