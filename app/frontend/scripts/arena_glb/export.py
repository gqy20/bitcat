import os
import math

import bpy

from .constants import FPS
from .fighter import create_fighter
from .animation import make_actions
from .primitives import clear_scene


def add_stage_helpers():
    bpy.ops.object.light_add(type="AREA", location=(0, -4, 5))
    light = bpy.context.object
    light.name = "Preview_Key_Light"
    light.data.energy = 450
    light.data.size = 4
    bpy.ops.object.camera_add(location=(0, -6, 2.2), rotation=(math.radians(74), 0, 0))
    bpy.context.scene.camera = bpy.context.object


def export_glb(path):
    bpy.context.scene.render.fps = FPS
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        export_animations=True,
        export_nla_strips=True,
        export_frame_range=False,
        export_apply=True,
    )


def build_variant(out_dir, filename, palette):
    clear_scene()
    root, nodes = create_fighter(filename.replace(".glb", ""), palette)
    make_actions(root, nodes)
    add_stage_helpers()
    os.makedirs(out_dir, exist_ok=True)
    export_glb(os.path.join(out_dir, filename))
