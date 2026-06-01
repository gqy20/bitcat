import math

from .primitives import (
    bevel_object,
    capsule,
    cone,
    cube,
    cylinder,
    empty,
    make_material,
    mix_color,
    sphere,
    talisman,
    torus,
)
from .details import (
    add_face_details,
    add_foot_details,
    add_hand_details,
    add_headwear_details,
    add_torso_details,
    add_weapon_details,
)


def create_fighter(name, palette):
    is_enemy = "enemy" in name.lower()
    mats = {
        "primary": make_material(f"{name}_primary", palette["primary"], roughness=0.36, metallic=0.02),
        "accent": make_material(f"{name}_accent", palette["accent"], roughness=0.34, metallic=0.05),
        "skin": make_material(f"{name}_skin", palette["skin"], roughness=0.68),
        "dark": make_material(f"{name}_dark", palette["dark"], roughness=0.42, metallic=0.02),
        "glow": make_material(f"{name}_glow", palette["glow"], emission=True, roughness=0.28, metallic=0.18),
        "metal": make_material(f"{name}_metal", palette.get("metal", (0.70, 0.76, 0.82, 1)), roughness=0.24, metallic=0.55),
        "leather": make_material(f"{name}_leather", palette.get("leather", (0.16, 0.08, 0.04, 1)), roughness=0.62),
        "scarf": make_material(f"{name}_scarf", palette.get("scarf", (0.95, 0.10, 0.18, 1)), roughness=0.72),
        "paper": make_material(f"{name}_paper", palette.get("paper", (1.0, 0.90, 0.62, 1)), roughness=0.82),
        "ink": make_material(f"{name}_ink", palette.get("ink", (0.32, 0.08, 0.06, 1)), roughness=0.74),
        "seal": make_material(f"{name}_seal", palette.get("seal", (0.86, 0.05, 0.05, 1)), roughness=0.64),
        "jade": make_material(f"{name}_jade", palette.get("jade", (0.30, 0.86, 0.70, 1)), roughness=0.38, metallic=0.08),
        "trim": make_material(f"{name}_trim", mix_color(palette["primary"], (1, 1, 1, 1), 0.42), roughness=0.36, metallic=0.10),
        "shadow": make_material(f"{name}_shadow", mix_color(palette["primary"], (0, 0, 0, 1), 0.46), roughness=0.58),
    }
    strand_mat = make_material(
        f"{name}_fur_strands",
        mix_color(palette["skin"], palette["dark"], 0.18),
        roughness=0.76,
        metallic=0.0,
    )

    root = empty(f"{name}_Root")
    body = empty("Body", (0, 0, 0), root)
    spine = empty("SpinePivot", (0, 0, 1.1), body)
    head_pivot = empty("HeadPivot", (0, 0, 1.68), spine)

    capsule("Hips", (0, 0, 0.68), 0.32, 0.66, mats["dark"], body, scale=(1.34, 0.86, 0.38))
    capsule("Torso", (0, 0, 1.10), 0.40, 0.86, mats["primary"], spine, scale=(1.00, 0.66, 0.90))
    bevel_object(cube("RobeFrontPanel", (0, -0.236, 1.26), (0.54, 0.052, 0.58), mats["accent"], spine), 0.020, 2)
    bevel_object(cube("RobeCollarL", (-0.16, -0.268, 1.50), (0.11, 0.034, 0.30), mats["paper"], spine), 0.008, 1)
    bevel_object(cube("RobeCollarR", (0.16, -0.268, 1.50), (0.11, 0.034, 0.30), mats["paper"], spine), 0.008, 1)
    bevel_object(cube("ChestPlate", (0, -0.275, 1.24), (0.46, 0.032, 0.22), mats["paper"], spine), 0.014, 2)
    bevel_object(cube("ChestPlateInset", (0, -0.298, 1.24), (0.34, 0.018, 0.14), mats["trim"], spine), 0.008, 1)
    bevel_object(cube("ChestGlow", (0, -0.312, 1.26), (0.30, 0.022, 0.044), mats["glow"], spine), 0.006, 1)
    bevel_object(cube("FishboneSpine", (0, -0.292, 1.30), (0.26, 0.018, 0.028), mats["glow"], spine), 0.004, 1)
    bevel_object(cube("FishboneHead", (0.16, -0.296, 1.30), (0.055, 0.018, 0.075), mats["glow"], spine), 0.004, 1)
    bevel_object(cube("FishboneTailA", (-0.16, -0.296, 1.335), (0.08, 0.016, 0.018), mats["glow"], spine), 0.003, 1)
    bevel_object(cube("FishboneTailB", (-0.16, -0.296, 1.265), (0.08, 0.016, 0.018), mats["glow"], spine), 0.003, 1)
    for idx, (x, z) in enumerate([(-0.20, 1.45), (0.20, 1.45), (-0.20, 1.11), (0.20, 1.11)]):
        sphere(f"ChestRivet{idx + 1}", (x, -0.292, z), (0.022, 0.012, 0.022), mats["metal"], spine)
    add_torso_details(spine, body, mats)
    talisman("ChestCharm", (0, -0.325, 1.02), spine, mats, scale=(0.13, 0.012, 0.28), glow=True)
    bevel_object(cube("TorsoTrimTop", (0, -0.244, 1.54), (0.58, 0.032, 0.055), mats["trim"], spine), 0.008, 1)
    bevel_object(cube("TorsoTrimLow", (0, -0.244, 0.96), (0.58, 0.032, 0.055), mats["shadow"], spine), 0.008, 1)
    bevel_object(cube("LeftChestStripe", (-0.23, -0.252, 1.24), (0.045, 0.03, 0.62), mats["trim"], spine), 0.006, 1)
    bevel_object(cube("RightChestStripe", (0.23, -0.252, 1.24), (0.045, 0.03, 0.62), mats["trim"], spine), 0.006, 1)
    bevel_object(cube("Belt", (0, -0.23, 0.80), (0.72, 0.07, 0.12), mats["leather"], body), 0.018, 1)
    torus("JadeBeltRing", (0, -0.286, 0.80), 0.070, 0.014, mats["jade"], body, rotation=(math.radians(90), 0, 0), scale=(1.0, 0.82, 1.0))
    cylinder("BellClapper", (0, -0.306, 0.745), 0.018, 0.040, mats["metal"], body, vertices=20, rotation=(math.radians(90), 0, 0))
    sphere("Bell", (0, -0.315, 0.705), (0.055, 0.046, 0.052), mats["metal"], body)
    for idx, x in enumerate([-0.29, -0.19, -0.09, 0.09, 0.19, 0.29]):
        sphere(f"BeltStud{idx + 1}", (x, -0.286, 0.81), (0.016, 0.009, 0.016), mats["metal"], body)
    bevel_object(cube("HipArmorL", (-0.34, -0.12, 0.66), (0.18, 0.11, 0.23), mats["metal"], body), 0.020, 2)
    bevel_object(cube("HipArmorR", (0.34, -0.12, 0.66), (0.18, 0.11, 0.23), mats["metal"], body), 0.020, 2)
    bevel_object(cube("TailBaseCollar", (0, 0.225, 0.82), (0.34, 0.075, 0.18), mats["metal"], body), 0.016, 1)
    cylinder("Neck", (0, 0, 1.67), 0.105, 0.18, mats["skin"], spine, vertices=18, rotation=(math.radians(90), 0, 0), scale=(1, 1, 0.9))
    sphere("Head", (0, -0.02, 1.96), (0.56, 0.470, 0.555), mats["skin"], head_pivot)
    for idx, (x, z, rot) in enumerate([(-0.18, 2.00, -8), (-0.10, 1.82, 4), (0.10, 1.82, -4), (0.18, 2.00, 8)]):
        cylinder(
            f"FaceFurStrand{idx + 1}",
            (x, -0.327, z),
            0.006,
            0.16,
            strand_mat,
            head_pivot,
            vertices=8,
            rotation=(0, math.radians(86 if x < 0 else 94), math.radians(rot)),
        )
    for idx, (x, z, rot) in enumerate([(-0.245, 2.070, -18), (-0.220, 1.875, -10), (0.245, 2.070, 18), (0.220, 1.875, 10)]):
        cylinder(
            f"SideFurLine{idx + 1}",
            (x, -0.465, z),
            0.005,
            0.165,
            strand_mat,
            head_pivot,
            vertices=8,
            rotation=(0, math.radians(86 if x < 0 else 94), math.radians(rot)),
        )
    capsule("HairCap", (0, -0.01, 2.25), 0.31, 0.22, mats["dark"], head_pivot, scale=(1.74, 1.12, 0.50))
    capsule("HairFringe", (0.05, -0.255, 2.12), 0.150, 0.42, mats["dark"], head_pivot, rotation=(0, 0, math.radians(84)), scale=(0.70, 0.95, 1.0))
    add_headwear_details(head_pivot, mats)
    bevel_object(cube("FaceShadow", (0, -0.350, 1.95), (0.30, 0.038, 0.058), mats["dark"], head_pivot), 0.006, 1)
    bevel_object(cube("SpiritEyeBand", (0, -0.395, 2.035), (0.46, 0.030, 0.092), mats["ink"], head_pivot), 0.012, 1)
    bevel_object(cube("SpiritEyeBandTopCord", (0, -0.415, 2.103), (0.55, 0.024, 0.032), mats["seal"], head_pivot), 0.006, 1)
    bevel_object(cube("SpiritEyeBandLowCord", (0, -0.415, 1.965), (0.55, 0.024, 0.032), mats["seal"], head_pivot), 0.006, 1)
    sphere("L_SpiritEye", (-0.125, -0.422, 2.035), (0.056, 0.012, 0.045), mats["glow"], head_pivot)
    sphere("R_SpiritEye", (0.125, -0.422, 2.035), (0.056, 0.012, 0.045), mats["glow"], head_pivot)
    add_face_details(head_pivot, mats)
    talisman("ForeheadCharm", (0, -0.430, 2.185), head_pivot, mats, scale=(0.085, 0.006, 0.205), glow=True)
    sphere("Nose", (0, -0.405, 1.89), (0.055, 0.026, 0.030), mats["dark"], head_pivot)
    sphere("L_CheekGlow", (-0.16, -0.410, 1.885), (0.034, 0.010, 0.022), mats["glow"], head_pivot)
    sphere("R_CheekGlow", (0.16, -0.410, 1.885), (0.034, 0.010, 0.022), mats["glow"], head_pivot)
    cone("L_CatEar", (-0.355, -0.01, 2.43), 0.22, 0.50, mats["dark"], head_pivot, vertices=3, rotation=(0, 0, math.radians(24)), scale=(1.02, 0.82, 1))
    cone("R_CatEar", (0.355, -0.01, 2.43), 0.22, 0.50, mats["dark"], head_pivot, vertices=3, rotation=(0, 0, math.radians(-24)), scale=(1.02, 0.82, 1))
    cone("L_InnerEar", (-0.355, -0.058, 2.405), 0.128, 0.315, mats["paper"], head_pivot, vertices=3, rotation=(0, 0, math.radians(24)), scale=(0.82, 0.62, 1))
    cone("R_InnerEar", (0.355, -0.058, 2.405), 0.128, 0.315, mats["paper"], head_pivot, vertices=3, rotation=(0, 0, math.radians(-24)), scale=(0.82, 0.62, 1))
    cylinder("L_WhiskerTop", (-0.18, -0.335, 1.96), 0.008, 0.30, mats["metal"], head_pivot, vertices=8, rotation=(0, math.radians(86), math.radians(8)))
    cylinder("R_WhiskerTop", (0.18, -0.335, 1.96), 0.008, 0.30, mats["metal"], head_pivot, vertices=8, rotation=(0, math.radians(94), math.radians(-8)))
    cylinder("L_WhiskerLow", (-0.18, -0.332, 1.91), 0.007, 0.26, mats["metal"], head_pivot, vertices=8, rotation=(0, math.radians(86), math.radians(-8)))
    cylinder("R_WhiskerLow", (0.18, -0.332, 1.91), 0.007, 0.26, mats["metal"], head_pivot, vertices=8, rotation=(0, math.radians(94), math.radians(8)))
    bevel_object(cube("ScarfBand", (0, -0.245, 1.62), (0.54, 0.065, 0.10), mats["scarf"], spine), 0.012, 1)
    for idx, x in enumerate([-0.20, -0.10, 0.0, 0.10, 0.20]):
        bevel_object(cube(f"ScarfStitch{idx + 1}", (x, -0.286, 1.62), (0.022, 0.018, 0.085), mats["trim"], spine), 0.003, 1)
    for idx, x in enumerate([-0.30, -0.18, -0.06, 0.06, 0.18, 0.30]):
        sphere(f"NecklaceBead{idx + 1}", (x, -0.318, 1.565 - abs(x) * 0.10), (0.024, 0.014, 0.024), mats["jade"], spine)
    sphere("NecklaceCenterGem", (0, -0.342, 1.500), (0.036, 0.014, 0.044), mats["glow"], spine)
    scarf = empty("ScarfTail", (-0.30, 0.18, 1.54), spine)
    bevel_object(cube("ScarfTailA", (0, 0, -0.18), (0.13, 0.055, 0.36), mats["scarf"], scarf), 0.012, 1)
    bevel_object(cube("ScarfTailB", (-0.03, 0.02, -0.48), (0.11, 0.05, 0.28), mats["scarf"], scarf), 0.010, 1)
    cape = empty("ShortCape", (0, 0.245, 1.44), spine)
    bevel_object(cube("CapeTopRoll", (0, 0, 0.08), (0.62, 0.075, 0.10), mats["scarf"], cape, local=True), 0.018, 2)
    for idx, x in enumerate([-0.22, 0.0, 0.22]):
        flap = bevel_object(cube(f"CapeFlap{idx + 1}", (x, 0.025, -0.24), (0.18, 0.045, 0.56), mats["scarf"], cape, local=True), 0.014, 2)
        flap.rotation_euler = (math.radians(5 + idx * 2), 0, math.radians((idx - 1) * 5))
        talisman(f"CapeTalisman{idx + 1}", (x, 0.060, -0.16), cape, mats, scale=(0.070, 0.008, 0.22), glow=idx == 1, local=True)
        bevel_object(cube(f"CapeGoldTrim{idx + 1}", (x, -0.006, -0.510), (0.158, 0.018, 0.030), mats["metal"], cape, local=True), 0.004, 1)
    for idx, x in enumerate([-0.31, 0.31]):
        side_flap = bevel_object(cube(f"CapeSideLayer{idx + 1}", (x, 0.042, -0.205), (0.115, 0.036, 0.470), mats["shadow"], cape, local=True), 0.010, 1)
        side_flap.rotation_euler = (math.radians(8), 0, math.radians(11 if x < 0 else -11))
    for idx, x in enumerate([-0.31, -0.155, 0.155, 0.31]):
        sphere(f"CapeHemBell{idx + 1}", (x, 0.010, -0.560), (0.026, 0.018, 0.030), mats["metal"], cape, local=True)
    torus("CapeJadeKnot", (0, -0.035, 0.06), 0.050, 0.010, mats["jade"], cape, rotation=(math.radians(90), 0, 0), local=True)
    for idx, x in enumerate([-0.25, 0.25]):
        talisman(f"BackShoulderCharm{idx + 1}", (x, 0.078, -0.04), cape, mats, scale=(0.075, 0.008, 0.25), glow=False, local=True)
    bevel_object(cube("RobeStrapL", (-0.24, -0.238, 1.16), (0.055, 0.045, 0.70), mats["leather"], spine), 0.008, 1)
    bevel_object(cube("RobeStrapR", (0.24, -0.238, 1.16), (0.055, 0.045, 0.70), mats["leather"], spine), 0.008, 1)
    talisman("BeltCharmL", (-0.26, -0.245, 0.80), body, mats, scale=(0.060, 0.006, 0.235), rotation=(0, 0, math.radians(-8)), glow=False)
    talisman("BeltCharmR", (0.26, -0.245, 0.80), body, mats, scale=(0.060, 0.006, 0.235), rotation=(0, 0, math.radians(8)), glow=False)
    bevel_object(cube("LeftBeltPouch", (-0.37, -0.18, 0.78), (0.16, 0.10, 0.16), mats["leather"], body), 0.018, 1)
    bevel_object(cube("RightBeltPouch", (0.37, -0.18, 0.78), (0.16, 0.10, 0.16), mats["leather"], body), 0.018, 1)

    nodes = {
        "root": root,
        "body": body,
        "spine": spine,
        "head": head_pivot,
        "scarf": scarf,
        "cape": cape,
    }

    for side, label in [(-1, "L"), (1, "R")]:
        limb_depth = -0.035 if label == "R" else 0.025
        foot_depth = -0.07 if label == "R" else 0.07
        shoulder = empty(f"{label}_Shoulder", (side * 0.59, limb_depth, 0.08), spine, local=True)
        elbow = empty(f"{label}_Elbow", (side * 0.035, 0, -0.60), shoulder, local=True)
        hip = empty(f"{label}_Hip", (side * 0.33, limb_depth, -0.08), body, local=True)
        knee = empty(f"{label}_Knee", (side * 0.035, foot_depth, -0.62), hip, local=True)

        sphere(f"{label}_ShoulderJoint", (side * -0.08, 0.00, -0.02), (0.15, 0.12, 0.15), mats["accent"], shoulder, local=True)
        bevel_object(cube(f"{label}_ShoulderPad", (side * 0.05, -0.02, 0.03), (0.32, 0.30, 0.18), mats["paper"], shoulder, local=True), 0.035, 2)
        bevel_object(cube(f"{label}_ShoulderStripe", (side * 0.05, -0.18, 0.045), (0.22, 0.035, 0.055), mats["seal"], shoulder, local=True), 0.006, 1)
        sphere(f"{label}_ShoulderRivet", (side * 0.16, -0.18, 0.035), (0.020, 0.012, 0.020), mats["metal"], shoulder, local=True)
        capsule(f"{label}_UpperArm", (side * 0.03, -0.005, -0.30), 0.128, 0.66, mats["accent"], shoulder, local=True, scale=(0.96, 0.88, 1.0))
        bevel_object(cube(f"{label}_UpperArmStripeA", (side * 0.04, -0.115, -0.20), (0.18, 0.035, 0.050), mats["trim"], shoulder, local=True), 0.006, 1)
        bevel_object(cube(f"{label}_UpperArmStripeB", (side * 0.04, -0.115, -0.40), (0.16, 0.035, 0.046), mats["shadow"], shoulder, local=True), 0.006, 1)
        bevel_object(cube(f"{label}_ElbowGuard", (side * 0.05, -0.105, -0.58), (0.22, 0.07, 0.13), mats["metal"], shoulder, local=True), 0.016, 1)
        capsule(f"{label}_LowerArm", (side * 0.045, -0.005, -0.30), 0.116, 0.62, mats["skin"], elbow, local=True, scale=(0.94, 0.86, 1.0))
        talisman(f"{label}_SleeveCharm", (side * 0.13, -0.132, -0.30), elbow, mats, scale=(0.050, 0.006, 0.24), glow=label == "R", local=True)
        sphere(f"{label}_Fist", (side * 0.055, -0.052, -0.66), (0.17, 0.145, 0.13), mats["leather"], elbow, local=True)
        sphere(f"{label}_Thumb", (side * 0.17, -0.105, -0.62), (0.045, 0.032, 0.065), mats["leather"], elbow, local=True)
        add_hand_details(label, side, elbow, mats)
        bevel_object(cube(f"{label}_WristBand", (0, -0.02, -0.40), (0.21, 0.22, 0.08), mats["metal"], elbow, local=True), 0.012, 1)

        sphere(f"{label}_HipJoint", (side * -0.045, 0.00, -0.02), (0.17, 0.12, 0.15), mats["primary"], hip, local=True)
        bevel_object(cube(f"{label}_HipConnector", (side * -0.070, -0.085, -0.05), (0.22, 0.07, 0.18), mats["metal"], hip, local=True), 0.016, 1)
        capsule(f"{label}_Thigh", (side * 0.025, 0, -0.31), 0.168, 0.68, mats["primary"], hip, local=True, scale=(0.98, 0.90, 1.0))
        bevel_object(cube(f"{label}_ThighPlate", (0, -0.145, -0.28), (0.27, 0.060, 0.39), mats["paper"], hip, local=True), 0.016, 1)
        bevel_object(cube(f"{label}_ThighSidePad", (side * 0.13, -0.035, -0.30), (0.080, 0.17, 0.35), mats["metal"], hip, local=True), 0.014, 1)
        bevel_object(cube(f"{label}_ThighBandTop", (0, -0.165, -0.12), (0.285, 0.044, 0.060), mats["leather"], hip, local=True), 0.008, 1)
        bevel_object(cube(f"{label}_ThighBandLow", (0, -0.165, -0.43), (0.260, 0.044, 0.055), mats["leather"], hip, local=True), 0.008, 1)
        capsule(f"{label}_Shin", (side * 0.020, 0, -0.27), 0.146, 0.62, mats["dark"], knee, local=True, scale=(0.88, 0.82, 1.0))
        bevel_object(cube(f"{label}_ShinGuard", (0, -0.135, -0.25), (0.23, 0.06, 0.38), mats["metal"], knee, local=True), 0.016, 1)
        bevel_object(cube(f"{label}_ShinGlow", (0, -0.178, -0.25), (0.105, 0.024, 0.26), mats["glow"], knee, local=True), 0.006, 1)
        talisman(f"{label}_ShinCharm", (side * 0.13, -0.120, -0.27), knee, mats, scale=(0.050, 0.006, 0.25), glow=label == "L", local=True)
        bevel_object(cube(f"{label}_KneeGuard", (0, -0.145, -0.04), (0.315, 0.080, 0.20), mats["jade"], knee, local=True), 0.018, 2)
        sphere(f"{label}_KneeRivetL", (-0.10, -0.185, -0.04), (0.018, 0.010, 0.018), mats["glow"], knee, local=True)
        sphere(f"{label}_KneeRivetR", (0.10, -0.185, -0.04), (0.018, 0.010, 0.018), mats["glow"], knee, local=True)
        bevel_object(cube(f"{label}_AnkleRing", (0, -0.055, -0.49), (0.285, 0.115, 0.08), mats["metal"], knee, local=True), 0.014, 1)
        bevel_object(cube(f"{label}_Foot", (side * 0.035, -0.20, -0.58), (0.48, 0.60, 0.20), mats["leather"], knee, local=True), 0.034, 3)
        bevel_object(cube(f"{label}_Sole", (side * 0.035, -0.225, -0.69), (0.50, 0.62, 0.060), mats["shadow"], knee, local=True), 0.014, 1)
        bevel_object(cube(f"{label}_ToeCap", (side * 0.035, -0.50, -0.56), (0.38, 0.15, 0.10), mats["metal"], knee, local=True), 0.016, 1)
        add_foot_details(label, knee, mats)
        for tread_idx, y in enumerate([-0.35, -0.23, -0.11]):
            bevel_object(cube(f"{label}_SoleTread{tread_idx + 1}", (side * 0.035, y, -0.725), (0.42, 0.035, 0.028), mats["metal"], knee, local=True), 0.006, 1)
        sphere(f"{label}_BootRivetL", (-0.11, -0.405, -0.49), (0.018, 0.010, 0.018), mats["glow"], knee, local=True)
        sphere(f"{label}_BootRivetR", (0.11, -0.405, -0.49), (0.018, 0.010, 0.018), mats["glow"], knee, local=True)

        nodes[f"{label}_shoulder"] = shoulder
        nodes[f"{label}_elbow"] = elbow
        nodes[f"{label}_hip"] = hip
        nodes[f"{label}_knee"] = knee

    weapon = empty("Weapon", (0.52, -0.20, -0.62), nodes["R_elbow"])
    weapon.rotation_euler = (math.radians(8), math.radians(-4), math.radians(-26))
    cylinder("SwordGrip", (0, 0, 0), 0.044, 0.42, mats["leather"], weapon, local=True, vertices=16, rotation=(math.radians(90), 0, 0))
    torus("SwordPommelCharmRing", (0, 0.20, 0.01), 0.055, 0.010, mats["jade"], weapon, local=True, rotation=(math.radians(90), 0, 0))
    bevel_object(cube("SwordPommelTasselA", (0.035, 0.255, -0.07), (0.028, 0.035, 0.18), mats["scarf"], weapon, local=True), 0.006, 1)
    bevel_object(cube("SwordPommelTasselB", (-0.035, 0.255, -0.07), (0.028, 0.035, 0.18), mats["scarf"], weapon, local=True), 0.006, 1)
    bevel_object(cube("SwordGuard", (0, -0.21, 0.03), (0.52, 0.07, 0.07), mats["jade"], weapon, local=True), 0.012, 1)
    sphere("SwordGem", (0, -0.185, 0.078), (0.038, 0.018, 0.038), mats["glow"], weapon, local=True)
    bevel_object(cube("SwordBladeBase", (0, -0.57, 0.08), (0.18, 0.74, 0.046), mats["paper"], weapon, local=True), 0.022, 2)
    bevel_object(cube("SwordBladeMid", (0, -1.00, 0.08), (0.14, 0.34, 0.040), mats["paper"], weapon, local=True), 0.018, 2)
    bevel_object(cube("SwordBladeTip", (0, -1.25, 0.08), (0.08, 0.22, 0.034), mats["paper"], weapon, local=True), 0.016, 2)
    bevel_object(cube("SwordBackPlate", (0, -0.82, 0.050), (0.060, 1.18, 0.025), mats["ink"], weapon, local=True), 0.006, 1)
    bevel_object(cube("SwordCoreGlow", (0, -0.82, 0.112), (0.052, 1.12, 0.016), mats["glow"], weapon, local=True), 0.004, 1)
    bevel_object(cube("SwordFaceInscriptionPanel", (0, -0.84, 0.136), (0.118, 0.420, 0.008), mats["paper"], weapon, local=True), 0.004, 1)
    bevel_object(cube("SwordEdgeGlowL", (-0.100, -0.82, 0.106), (0.020, 0.96, 0.012), mats["glow"], weapon, local=True), 0.003, 1)
    bevel_object(cube("SwordEdgeGlowR", (0.100, -0.82, 0.106), (0.020, 0.96, 0.012), mats["glow"], weapon, local=True), 0.003, 1)
    bevel_object(cube("SwordEnergyFinL", (-0.155, -0.82, 0.088), (0.026, 0.64, 0.020), mats["glow"], weapon, local=True), 0.004, 1)
    bevel_object(cube("SwordEnergyFinR", (0.155, -0.82, 0.088), (0.026, 0.64, 0.020), mats["glow"], weapon, local=True), 0.004, 1)
    bevel_object(cube("SwordGoldEdgeL", (-0.132, -0.81, 0.075), (0.018, 1.10, 0.018), mats["metal"], weapon, local=True), 0.003, 1)
    bevel_object(cube("SwordGoldEdgeR", (0.132, -0.81, 0.075), (0.018, 1.10, 0.018), mats["metal"], weapon, local=True), 0.003, 1)
    for idx, x in enumerate([-0.16, 0.16]):
        torus(f"SwordSideJade{idx + 1}", (x, -0.31, 0.082), 0.035, 0.006, mats["jade"], weapon, local=True, rotation=(math.radians(90), 0, 0), scale=(0.72, 1.0, 1.0))
        talisman(f"SwordGuardSeal{idx + 1}", (x * 1.05, -0.42, -0.065), weapon, mats, scale=(0.040, 0.005, 0.145), glow=idx == 0, local=True)
    add_weapon_details(weapon, mats)
    for idx, y in enumerate([-0.45, -0.72, -0.99]):
        bevel_object(cube(f"SwordEtch{idx + 1}", (0, y, 0.126), (0.095, 0.020, 0.010), mats["glow"], weapon, local=True), 0.002, 1)
    talisman("SwordHangingCharm", (0.16, -0.28, -0.06), weapon, mats, scale=(0.050, 0.006, 0.20), glow=True, local=True)
    for idx, y in enumerate([-0.58, -0.70, -0.82, -0.94]):
        bevel_object(cube(f"SwordTinyRune{idx + 1}", (0, y, 0.148), (0.055, 0.010, 0.008), mats["seal"], weapon, local=True), 0.0015, 1)
    nodes["weapon"] = weapon
    offhand = empty("OffhandDagger", (-0.42, -0.16, -0.64), nodes["L_elbow"])
    offhand.rotation_euler = (math.radians(4), 0, math.radians(30))
    cylinder("DaggerGrip", (0, 0, 0), 0.032, 0.28, mats["leather"], offhand, local=True, vertices=14, rotation=(math.radians(90), 0, 0))
    bevel_object(cube("DaggerPommel", (0, 0.13, 0.01), (0.09, 0.05, 0.09), mats["metal"], offhand, local=True), 0.010, 1)
    bevel_object(cube("DaggerGuard", (0, -0.15, 0.03), (0.30, 0.052, 0.052), mats["jade"], offhand, local=True), 0.008, 1)
    bevel_object(cube("DaggerBlade", (0, -0.36, 0.07), (0.076, 0.32, 0.026), mats["paper"], offhand, local=True), 0.010, 1)
    bevel_object(cube("DaggerSpine", (0, -0.36, 0.052), (0.024, 0.29, 0.012), mats["shadow"], offhand, local=True), 0.003, 1)
    bevel_object(cube("DaggerGlow", (0, -0.36, 0.094), (0.020, 0.25, 0.010), mats["glow"], offhand, local=True), 0.002, 1)
    nodes["offhand"] = offhand
    tail = empty("Tail", (0, 0.18, 0.86), body)
    for idx, (x, y, z, rot_z, scale) in enumerate([
        (0.00, 0.04, 0.10, 18, 1.0),
        (0.06, 0.08, 0.32, 28, 0.86),
        (0.13, 0.10, 0.52, 42, 0.70),
    ]):
        cylinder(
            f"TailSegment{idx + 1}",
            (x, y, z),
            0.055 * scale,
            0.30,
            mats["dark"],
            tail,
            vertices=14,
            rotation=(math.radians(28), 0, math.radians(rot_z)),
            scale=(0.86, 0.86, 1),
        )
    for idx, (x, y, z, scale) in enumerate([(0.04, 0.055, 0.24, 0.95), (0.11, 0.095, 0.45, 0.74)]):
        cylinder(
            f"TailRing{idx + 1}",
            (x, y, z),
            0.064 * scale,
            0.034,
            mats["glow"],
            tail,
            vertices=14,
            rotation=(math.radians(28), 0, math.radians(28 + idx * 12)),
            scale=(0.90, 0.90, 1),
        )
    sphere("TailGlowTip", (0.18, 0.13, 0.64), (0.050, 0.038, 0.050), mats["glow"], tail, local=True)
    nodes["tail"] = tail

    root["arena_model"] = "bitcat_eastern_spirit_warrior_v1"
    return root, nodes
