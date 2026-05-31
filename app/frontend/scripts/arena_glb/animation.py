import math

import bpy

from .constants import CLIPS


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
    idle_pose = {
        "rot": {
            "body": (0, 0, -0.035),
            "spine": (-0.04, -0.10, 0.035),
            "head": (0.02, -0.08, 0),
            "L_shoulder": (-0.46, 0.02, -0.12),
            "R_shoulder": (-0.58, -0.04, 0.18),
            "L_elbow": (-0.26, 0.04, -0.10),
            "R_elbow": (-0.22, -0.03, 0.10),
            "L_hip": (0.04, 0.02, -0.03),
            "R_hip": (-0.04, -0.03, 0.04),
            "L_knee": (0.10, 0, -0.01),
            "R_knee": (0.06, 0, 0.01),
            "weapon": (math.radians(8), 0, math.radians(-34)),
            "offhand": (math.radians(4), 0, math.radians(44)),
            "tail": (0.04, 0, 0.20),
            "scarf": (0.04, 0, -0.10),
            "cape": (0.06, 0, 0.03),
        }
    }
    swing_a = {
        "rot": {
            "weapon": (math.radians(6), 0, math.radians(-22)),
            "offhand": (math.radians(4), 0, math.radians(30)),
            "L_shoulder": (0.68, 0, -0.18),
            "R_shoulder": (-0.68, 0, 0.22),
            "L_hip": (-0.5, 0, 0),
            "R_hip": (0.5, 0, 0),
            "L_knee": (0.18, 0, 0),
            "R_knee": (0.32, 0, 0),
        },
        "loc": {"body": (0, 0, 0.03)},
    }
    swing_b = {
        "rot": {
            "weapon": (math.radians(10), 0, math.radians(-30)),
            "offhand": (math.radians(4), 0, math.radians(38)),
            "L_shoulder": (-0.68, 0, -0.24),
            "R_shoulder": (0.68, 0, 0.24),
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
            "weapon": (math.radians(10), 0, math.radians(-44)),
            "offhand": (math.radians(6), 0, math.radians(52)),
        },
        "loc": {"body": (0, 0.02, -0.02)},
    }

    create_clip(root, nodes, "Idle", [
        (0, idle_pose),
        (18, {"loc": {"body": (0, 0, 0.035)}, "rot": {**idle_pose["rot"], "head": (-0.04, 0, 0), "tail": (0.08, 0, 0.24), "scarf": (0.03, 0, -0.12), "cape": (0.11, 0, -0.04)}}),
        (36, idle_pose),
        (47, idle_pose),
    ])
    create_clip(root, nodes, "Run", [(0, swing_a), (8, swing_b), (16, swing_a), (24, swing_b)])
    create_clip(root, nodes, "Jump", [
        (0, idle_pose),
        (8, {"loc": {"body": (0, 0, 0.18)}, "rot": {"L_shoulder": (-0.85, 0, -0.2), "R_shoulder": (-0.72, 0, 0.2), "L_hip": (0.32, 0, 0), "R_hip": (-0.22, 0, 0), "spine": (-0.08, 0, 0), "weapon": (math.radians(18), 0, math.radians(-30)), "offhand": (math.radians(8), 0, math.radians(42))}}),
        (22, {"loc": {"body": (0, 0, 0.08)}, "rot": {"L_hip": (0.12, 0, 0), "R_hip": (-0.12, 0, 0), "weapon": (math.radians(10), 0, math.radians(-24)), "offhand": (math.radians(4), 0, math.radians(34))}}),
        (30, idle_pose),
    ])
    create_clip(root, nodes, "LightPunch", [
        (0, idle_pose),
        (4, {"rot": {"R_shoulder": (0.58, -0.08, -0.32), "R_elbow": (-0.46, 0, -0.10), "L_shoulder": (-0.28, 0, 0.20), "spine": (0.08, 0.24, 0), "weapon": (math.radians(-20), math.radians(-8), math.radians(-82)), "offhand": (math.radians(8), 0, math.radians(54))}, "loc": {"body": (-0.04, 0, -0.018)}}),
        (9, {"rot": {"R_shoulder": (-1.82, -0.20, 0.76), "R_elbow": (0.22, 0.04, 0.22), "L_shoulder": (0.20, 0, -0.40), "L_elbow": (-0.25, 0, -0.20), "weapon": (0.72, -0.22, 1.42), "offhand": (0, 0, -0.42), "spine": (-0.16, -0.34, 0), "head": (0, -0.13, 0), "tail": (-0.22, 0, -0.48), "scarf": (0.15, 0, 0.48), "cape": (0.24, 0, 0.42)}, "loc": {"body": (0.075, 0, 0.024)}}),
        (15, {"rot": {"R_shoulder": (-0.88, 0, 0.26), "R_elbow": (-0.12, 0, 0.05), "L_shoulder": (-0.18, 0, -0.18), "weapon": (math.radians(18), math.radians(-4), math.radians(-10)), "spine": (-0.03, -0.06, 0)}}),
        (21, idle_pose),
        (24, idle_pose),
    ])
    create_clip(root, nodes, "HeavyKick", [
        (0, idle_pose),
        (9, {"rot": {"R_hip": (0.58, 0.08, 0), "R_knee": (0.62, 0, 0), "L_shoulder": (-0.42, 0, -0.2), "R_shoulder": (0.48, -0.08, -0.26), "spine": (0.1, 0.18, 0), "weapon": (math.radians(-12), math.radians(-6), math.radians(-72)), "offhand": (math.radians(4), 0, math.radians(44))}}),
        (17, {"rot": {"R_hip": (-1.18, 0.28, 0), "R_knee": (-0.1, 0, 0), "L_shoulder": (-0.6, 0, -0.35), "R_shoulder": (-0.18, -0.16, 0.44), "spine": (0.05, -0.32, 0), "tail": (-0.12, 0, -0.48), "scarf": (0.08, 0, 0.34), "cape": (0.18, 0, 0.28), "weapon": (math.radians(42), math.radians(-12), math.radians(66)), "offhand": (math.radians(6), 0, math.radians(52))}}),
        (30, idle_pose),
        (37, idle_pose),
    ])
    create_clip(root, nodes, "Guard", [(0, guard), (18, guard), (36, guard)])
    create_clip(root, nodes, "Hurt", [
        (0, idle_pose),
        (5, {"rot": {"spine": (0.44, 0.12, 0), "head": (0.34, 0.08, 0), "L_shoulder": (0.78, 0, -0.18), "R_shoulder": (0.72, 0, 0.16), "L_elbow": (0.20, 0, -0.10), "R_elbow": (0.24, 0, 0.10), "tail": (0.38, 0, 0.52), "scarf": (-0.14, 0, -0.38), "cape": (-0.16, 0, -0.28), "weapon": (math.radians(28), 0, math.radians(-78)), "offhand": (math.radians(14), 0, math.radians(80))}, "loc": {"body": (-0.06, 0.05, -0.045)}}),
        (14, {"rot": {"spine": (0.16, 0.04, 0), "head": (0.10, 0.02, 0), "L_shoulder": (0.34, 0, -0.10), "R_shoulder": (0.34, 0, 0.10), "weapon": (math.radians(18), 0, math.radians(-54)), "offhand": (math.radians(8), 0, math.radians(60))}, "loc": {"body": (-0.02, 0.02, -0.02)}}),
        (24, idle_pose),
        (29, idle_pose),
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
