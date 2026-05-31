import argparse
import math
import os
import sys

import bpy


FPS = 30
CLIPS = {
    "Idle": (1, 48),
    "Run": (60, 84),
    "Jump": (100, 130),
    "LightPunch": (150, 174),
    "HeavyKick": (195, 232),
    "Guard": (250, 286),
    "Hurt": (305, 334),
    "Dead": (355, 394),
    "Win": (420, 462),
}


def clear_scene():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete()


def make_material(name, color, emission=False):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = color
    mat.use_nodes = True
    bsdf = next((node for node in mat.node_tree.nodes if node.type == "BSDF_PRINCIPLED"), None)
    if bsdf:
        if "Base Color" in bsdf.inputs:
            bsdf.inputs["Base Color"].default_value = color
        if "Roughness" in bsdf.inputs:
            bsdf.inputs["Roughness"].default_value = 0.62
        if emission and "Emission Color" in bsdf.inputs:
            bsdf.inputs["Emission Color"].default_value = color
        if emission and "Emission Strength" in bsdf.inputs:
            bsdf.inputs["Emission Strength"].default_value = 0.35
    return mat


def empty(name, loc=(0, 0, 0), parent=None):
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = "PLAIN_AXES"
    obj.empty_display_size = 0.12
    obj.location = loc
    bpy.context.collection.objects.link(obj)
    if parent:
        obj.parent = parent
    return obj


def cube(name, loc, scale, mat, parent=None):
    bpy.ops.mesh.primitive_cube_add(size=1, location=loc)
    obj = bpy.context.object
    obj.name = name
    obj.dimensions = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    if parent:
        set_world_parent(obj, parent)
    return obj


def sphere(name, loc, scale, mat, parent=None):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=18, ring_count=10, radius=1, location=loc)
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    if parent:
        set_world_parent(obj, parent)
    return obj


def set_world_parent(child, parent):
    matrix = child.matrix_world.copy()
    child.parent = parent
    child.matrix_parent_inverse = parent.matrix_world.inverted()
    child.matrix_world = matrix


def create_fighter(name, palette):
    mats = {
        "primary": make_material(f"{name}_primary", palette["primary"]),
        "accent": make_material(f"{name}_accent", palette["accent"]),
        "skin": make_material(f"{name}_skin", palette["skin"]),
        "dark": make_material(f"{name}_dark", palette["dark"]),
        "glow": make_material(f"{name}_glow", palette["glow"], emission=True),
    }

    root = empty(f"{name}_Root")
    body = empty("Body", (0, 0, 0), root)
    spine = empty("SpinePivot", (0, 0, 1.1), body)
    head_pivot = empty("HeadPivot", (0, 0, 1.68), spine)

    cube("Hips", (0, 0, 0.72), (0.66, 0.42, 0.30), mats["dark"], body)
    cube("Torso", (0, 0, 1.15), (0.78, 0.46, 0.9), mats["primary"], spine)
    cube("ChestGlow", (0, -0.245, 1.28), (0.42, 0.04, 0.09), mats["glow"], spine)
    cube("Belt", (0, -0.25, 0.78), (0.76, 0.055, 0.11), mats["glow"], body)
    cube("Emblem", (0, -0.272, 1.28), (0.18, 0.035, 0.18), mats["glow"], spine)
    cube("Neck", (0, 0, 1.67), (0.22, 0.2, 0.16), mats["skin"], spine)
    sphere("Head", (0, -0.02, 1.94), (0.30, 0.29, 0.34), mats["skin"], head_pivot)
    cube("Hair", (0, -0.01, 2.17), (0.48, 0.35, 0.16), mats["dark"], head_pivot)
    cube("Face", (0, -0.31, 1.96), (0.22, 0.04, 0.06), mats["dark"], head_pivot)
    cube("Visor", (0, -0.335, 2.02), (0.34, 0.035, 0.08), mats["glow"], head_pivot)

    nodes = {
        "root": root,
        "body": body,
        "spine": spine,
        "head": head_pivot,
    }

    for side, label in [(-1, "L"), (1, "R")]:
        shoulder = empty(f"{label}_Shoulder", (side * 0.52, 0, 1.48), spine)
        elbow = empty(f"{label}_Elbow", (0, 0, -0.55), shoulder)
        hip = empty(f"{label}_Hip", (side * 0.22, 0, 0.66), body)
        knee = empty(f"{label}_Knee", (0, 0, -0.56), hip)

        cube(f"{label}_ShoulderPad", (side * 0.05, -0.02, 0.03), (0.34, 0.32, 0.20), mats["dark"], shoulder)
        cube(f"{label}_UpperArm", (0, 0, -0.27), (0.22, 0.22, 0.58), mats["accent"], shoulder)
        cube(f"{label}_LowerArm", (0, 0, -0.27), (0.20, 0.20, 0.54), mats["skin"], elbow)
        cube(f"{label}_Fist", (0, -0.04, -0.58), (0.27, 0.25, 0.22), mats["glow"], elbow)
        cube(f"{label}_WristBand", (0, -0.02, -0.40), (0.22, 0.24, 0.09), mats["glow"], elbow)

        cube(f"{label}_Thigh", (0, 0, -0.28), (0.26, 0.25, 0.58), mats["primary"], hip)
        cube(f"{label}_Shin", (0, 0, -0.25), (0.23, 0.23, 0.52), mats["dark"], knee)
        cube(f"{label}_KneeGuard", (0, -0.13, -0.06), (0.26, 0.06, 0.16), mats["glow"], knee)
        cube(f"{label}_Foot", (side * 0.02, -0.12, -0.54), (0.36, 0.52, 0.17), mats["glow"], knee)

        nodes[f"{label}_shoulder"] = shoulder
        nodes[f"{label}_elbow"] = elbow
        nodes[f"{label}_hip"] = hip
        nodes[f"{label}_knee"] = knee

    root["arena_model"] = "object_rig_v2"
    return root, nodes


def key_object(obj, frame):
    obj.keyframe_insert(data_path="location", frame=frame)
    obj.keyframe_insert(data_path="rotation_euler", frame=frame)
    obj.keyframe_insert(data_path="scale", frame=frame)


def reset_nodes(nodes):
    for obj in nodes.values():
        obj.location.x = obj.location.x
        obj.rotation_euler = (0, 0, 0)
        obj.scale = (1, 1, 1)


def pose(nodes, values):
    for name, rot in values.get("rot", {}).items():
        nodes[name].rotation_euler = rot
    for name, loc in values.get("loc", {}).items():
        nodes[name].location = loc
    for name, scale in values.get("scale", {}).items():
        nodes[name].scale = scale


def key_pose(nodes, frame, values=None):
    reset_nodes(nodes)
    if values:
        pose(nodes, values)
    bpy.context.scene.frame_set(frame)
    for obj in nodes.values():
        key_object(obj, frame)


def create_clip(root, nodes, name, frames):
    start, end = CLIPS[name]
    root.animation_data_clear()
    for frame, values in frames:
        key_pose(nodes, start + frame, values)
    action = root.animation_data.action
    if not action:
        return
    action.name = name
    action.use_fake_user = True
    nla = root.animation_data.nla_tracks.new()
    nla.name = name
    strip = nla.strips.new(name, start, action)
    strip.frame_end = end
    strip.action_frame_start = start
    strip.action_frame_end = end


def make_actions(root, nodes):
    swing_a = {
        "rot": {
            "L_shoulder": (0.68, 0, 0.08),
            "R_shoulder": (-0.68, 0, -0.08),
            "L_hip": (-0.5, 0, 0),
            "R_hip": (0.5, 0, 0),
            "L_knee": (0.18, 0, 0),
            "R_knee": (0.32, 0, 0),
        },
        "loc": {"body": (0, 0, 0.03)},
    }
    swing_b = {
        "rot": {
            "L_shoulder": (-0.68, 0, 0.08),
            "R_shoulder": (0.68, 0, -0.08),
            "L_hip": (0.5, 0, 0),
            "R_hip": (-0.5, 0, 0),
            "L_knee": (0.32, 0, 0),
            "R_knee": (0.18, 0, 0),
        },
        "loc": {"body": (0, 0, 0.02)},
    }
    guard = {
        "rot": {
            "spine": (0.12, 0, 0),
            "L_shoulder": (-1.18, 0, -0.36),
            "R_shoulder": (-1.18, 0, 0.36),
            "L_elbow": (-0.55, 0, 0),
            "R_elbow": (-0.55, 0, 0),
        },
        "loc": {"body": (0, 0.02, -0.02)},
    }

    create_clip(root, nodes, "Idle", [
        (0, {}),
        (18, {"loc": {"body": (0, 0, 0.035)}, "rot": {"head": (-0.04, 0, 0)}}),
        (36, {}),
        (47, {}),
    ])
    create_clip(root, nodes, "Run", [(0, swing_a), (8, swing_b), (16, swing_a), (24, swing_b)])
    create_clip(root, nodes, "Jump", [
        (0, {}),
        (8, {"loc": {"body": (0, 0, 0.18)}, "rot": {"L_shoulder": (-0.85, 0, -0.2), "R_shoulder": (-0.72, 0, 0.2), "L_hip": (0.32, 0, 0), "R_hip": (-0.22, 0, 0), "spine": (-0.08, 0, 0)}}),
        (22, {"loc": {"body": (0, 0, 0.08)}, "rot": {"L_hip": (0.12, 0, 0), "R_hip": (-0.12, 0, 0)}}),
        (30, {}),
    ])
    create_clip(root, nodes, "LightPunch", [
        (0, {}),
        (6, {"rot": {"R_shoulder": (0.55, 0, -0.2), "R_elbow": (-0.25, 0, 0), "L_shoulder": (-0.2, 0, 0.25), "spine": (0.04, 0.18, 0)}}),
        (11, {"rot": {"R_shoulder": (-1.42, 0, -0.26), "R_elbow": (0.05, 0, 0), "L_shoulder": (0.25, 0, 0.2), "spine": (-0.08, -0.22, 0), "head": (0, -0.12, 0)}}),
        (20, {}),
        (24, {}),
    ])
    create_clip(root, nodes, "HeavyKick", [
        (0, {}),
        (9, {"rot": {"R_hip": (0.58, 0.08, 0), "R_knee": (0.62, 0, 0), "L_shoulder": (-0.42, 0, -0.2), "spine": (0.1, 0.18, 0)}}),
        (17, {"rot": {"R_hip": (-1.18, 0.28, 0), "R_knee": (-0.1, 0, 0), "L_shoulder": (-0.6, 0, -0.35), "R_shoulder": (0.35, 0, 0.2), "spine": (0.05, -0.32, 0)}}),
        (30, {}),
        (37, {}),
    ])
    create_clip(root, nodes, "Guard", [(0, guard), (18, guard), (36, guard)])
    create_clip(root, nodes, "Hurt", [
        (0, {}),
        (7, {"rot": {"spine": (0.38, 0, 0), "head": (0.28, 0, 0), "L_shoulder": (0.65, 0, -0.1), "R_shoulder": (0.65, 0, 0.1)}, "loc": {"body": (0, 0.04, -0.03)}}),
        (22, {}),
        (29, {}),
    ])
    create_clip(root, nodes, "Dead", [
        (0, {"rot": {"spine": (0.35, 0, 0)}}),
        (18, {"rot": {"body": (0, 1.15, 0), "L_shoulder": (0.7, 0, 0), "R_shoulder": (0.7, 0, 0)}}),
        (39, {"rot": {"body": (0, 1.35, 0), "L_shoulder": (0.8, 0, 0), "R_shoulder": (0.8, 0, 0)}}),
    ])
    create_clip(root, nodes, "Win", [
        (0, {}),
        (10, {"rot": {"L_shoulder": (-2.15, 0, -0.12), "R_shoulder": (-2.15, 0, 0.12)}, "loc": {"body": (0, 0, 0.05)}}),
        (24, {"rot": {"L_shoulder": (-1.8, 0, -0.18), "R_shoulder": (-2.25, 0, 0.18)}, "loc": {"body": (0, 0, 0.02)}}),
        (42, {"rot": {"L_shoulder": (-2.15, 0, -0.12), "R_shoulder": (-2.15, 0, 0.12)}}),
    ])


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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out-dir", required=True)
    parser.add_argument("--variant", default="all", choices=["all", "player", "enemy"])
    argv = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else sys.argv[1:]
    args = parser.parse_args(argv)

    variants = {
        "player": {
            "primary": (0.03, 0.32, 1.0, 1),
            "accent": (0.32, 0.78, 1.0, 1),
            "skin": (1.0, 0.76, 0.58, 1),
            "dark": (0.04, 0.07, 0.12, 1),
            "glow": (0.50, 0.95, 1.0, 1),
        },
        "enemy": {
            "primary": (0.78, 0.12, 0.26, 1),
            "accent": (1.0, 0.45, 0.42, 1),
            "skin": (0.95, 0.70, 0.55, 1),
            "dark": (0.14, 0.04, 0.10, 1),
            "glow": (1.0, 0.72, 0.22, 1),
        },
    }
    selected = variants.keys() if args.variant == "all" else [args.variant]
    for variant in selected:
        build_variant(args.out_dir, f"{variant}.glb", variants[variant])


if __name__ == "__main__":
    main()
