import math

from .primitives import bevel_object, cone, cube, cylinder, sphere, talisman, torus


def add_face_details(head_pivot, mats):
    bevel_object(cube("MaskBridge", (0, -0.500, 2.040), (0.070, 0.012, 0.120), mats["metal"], head_pivot), 0.004, 1)
    bevel_object(cube("L_MaskPlate", (-0.154, -0.502, 2.040), (0.190, 0.012, 0.104), mats["accent"], head_pivot), 0.010, 1).rotation_euler.z = math.radians(-5)
    bevel_object(cube("R_MaskPlate", (0.154, -0.502, 2.040), (0.190, 0.012, 0.104), mats["accent"], head_pivot), 0.010, 1).rotation_euler.z = math.radians(5)
    bevel_object(cube("L_MaskGoldTop", (-0.154, -0.515, 2.104), (0.198, 0.010, 0.020), mats["metal"], head_pivot), 0.003, 1).rotation_euler.z = math.radians(-5)
    bevel_object(cube("R_MaskGoldTop", (0.154, -0.515, 2.104), (0.198, 0.010, 0.020), mats["metal"], head_pivot), 0.003, 1).rotation_euler.z = math.radians(5)
    bevel_object(cube("L_MaskGoldLow", (-0.154, -0.515, 1.976), (0.198, 0.010, 0.020), mats["metal"], head_pivot), 0.003, 1).rotation_euler.z = math.radians(5)
    bevel_object(cube("R_MaskGoldLow", (0.154, -0.515, 1.976), (0.198, 0.010, 0.020), mats["metal"], head_pivot), 0.003, 1).rotation_euler.z = math.radians(-5)
    sphere("L_EyeWhite", (-0.150, -0.455, 2.040), (0.112, 0.014, 0.070), mats["paper"], head_pivot)
    sphere("R_EyeWhite", (0.150, -0.455, 2.040), (0.112, 0.014, 0.070), mats["paper"], head_pivot)
    sphere("L_EyeCore", (-0.145, -0.472, 2.026), (0.060, 0.009, 0.046), mats["glow"], head_pivot)
    sphere("R_EyeCore", (0.145, -0.472, 2.026), (0.060, 0.009, 0.046), mats["glow"], head_pivot)
    sphere("L_EyePupil", (-0.125, -0.484, 2.018), (0.024, 0.004, 0.028), mats["dark"], head_pivot)
    sphere("R_EyePupil", (0.170, -0.484, 2.018), (0.024, 0.004, 0.028), mats["dark"], head_pivot)
    sphere("L_EyeSpark", (-0.180, -0.492, 2.068), (0.016, 0.003, 0.014), mats["paper"], head_pivot)
    sphere("R_EyeSpark", (0.105, -0.492, 2.068), (0.016, 0.003, 0.014), mats["paper"], head_pivot)
    bevel_object(cube("L_AngryBrow", (-0.150, -0.488, 2.138), (0.170, 0.016, 0.030), mats["dark"], head_pivot), 0.004, 1).rotation_euler.z = math.radians(-14)
    bevel_object(cube("R_AngryBrow", (0.150, -0.488, 2.138), (0.170, 0.016, 0.030), mats["dark"], head_pivot), 0.004, 1).rotation_euler.z = math.radians(14)
    sphere("MuzzlePad", (0, -0.445, 1.872), (0.155, 0.018, 0.064), mats["paper"], head_pivot)
    bevel_object(cube("MouthMark", (0, -0.470, 1.838), (0.112, 0.010, 0.016), mats["ink"], head_pivot), 0.003, 1)
    torus("L_JadeHairBead", (-0.292, -0.06, 2.19), 0.040, 0.009, mats["jade"], head_pivot, rotation=(math.radians(90), 0, 0))
    torus("R_JadeHairBead", (0.292, -0.06, 2.19), 0.040, 0.009, mats["jade"], head_pivot, rotation=(math.radians(90), 0, 0))
    torus("L_EarRing", (-0.325, -0.045, 2.285), 0.046, 0.007, mats["metal"], head_pivot, rotation=(math.radians(90), 0, math.radians(12)), scale=(0.72, 1.0, 1.0))
    torus("R_EarRing", (0.325, -0.045, 2.285), 0.046, 0.007, mats["metal"], head_pivot, rotation=(math.radians(90), 0, math.radians(-12)), scale=(0.72, 1.0, 1.0))
    sphere("L_EarJadeDrop", (-0.350, -0.052, 2.205), (0.024, 0.014, 0.032), mats["jade"], head_pivot)
    sphere("R_EarJadeDrop", (0.350, -0.052, 2.205), (0.024, 0.014, 0.032), mats["jade"], head_pivot)
    torus("L_MaskCurl", (-0.245, -0.485, 2.060), 0.068, 0.007, mats["glow"], head_pivot, rotation=(math.radians(90), 0, math.radians(22)), scale=(0.92, 0.50, 1.0))
    torus("R_MaskCurl", (0.245, -0.485, 2.060), 0.068, 0.007, mats["glow"], head_pivot, rotation=(math.radians(90), 0, math.radians(-22)), scale=(0.92, 0.50, 1.0))
    bevel_object(cube("L_CheekStripe", (-0.230, -0.485, 1.895), (0.135, 0.010, 0.024), mats["paper"], head_pivot), 0.003, 1).rotation_euler.z = math.radians(-12)
    bevel_object(cube("R_CheekStripe", (0.230, -0.485, 1.895), (0.135, 0.010, 0.024), mats["paper"], head_pivot), 0.003, 1).rotation_euler.z = math.radians(12)
    bevel_object(cube("L_FacePaintHook", (-0.306, -0.492, 1.990), (0.048, 0.010, 0.170), mats["glow"], head_pivot), 0.003, 1).rotation_euler.z = math.radians(-24)
    bevel_object(cube("R_FacePaintHook", (0.306, -0.492, 1.990), (0.048, 0.010, 0.170), mats["glow"], head_pivot), 0.003, 1).rotation_euler.z = math.radians(24)
    cone("L_CheekFurFanTop", (-0.405, -0.448, 1.990), 0.070, 0.190, mats["paper"], head_pivot, vertices=3, rotation=(0, math.radians(84), math.radians(-22)), scale=(0.70, 1.0, 1.0))
    cone("L_CheekFurFanLow", (-0.405, -0.448, 1.900), 0.060, 0.165, mats["paper"], head_pivot, vertices=3, rotation=(0, math.radians(84), math.radians(-40)), scale=(0.70, 1.0, 1.0))
    cone("R_CheekFurFanTop", (0.405, -0.448, 1.990), 0.070, 0.190, mats["paper"], head_pivot, vertices=3, rotation=(0, math.radians(96), math.radians(22)), scale=(0.70, 1.0, 1.0))
    cone("R_CheekFurFanLow", (0.405, -0.448, 1.900), 0.060, 0.165, mats["paper"], head_pivot, vertices=3, rotation=(0, math.radians(96), math.radians(40)), scale=(0.70, 1.0, 1.0))
    for idx, x in enumerate([-0.225, -0.155, 0.155, 0.225]):
        sphere(f"MaskPearl{idx + 1}", (x, -0.498, 2.130), (0.018, 0.005, 0.018), mats["jade"], head_pivot)
    for idx, (x, z) in enumerate([(-0.080, 1.815), (0.080, 1.815)]):
        bevel_object(cube(f"MouthFang{idx + 1}", (x, -0.482, z), (0.028, 0.010, 0.052), mats["paper"], head_pivot), 0.004, 1)


def add_headwear_details(head_pivot, mats):
    cylinder("CrownBase", (0, -0.022, 2.385), 0.155, 0.110, mats["metal"], head_pivot, vertices=24, rotation=(math.radians(90), 0, 0), scale=(1.36, 0.80, 1.0))
    torus("CrownJadeRing", (0, -0.084, 2.390), 0.126, 0.014, mats["jade"], head_pivot, rotation=(math.radians(90), 0, 0), scale=(1.22, 0.72, 1.0))
    sphere("CrownGem", (0, -0.150, 2.405), (0.044, 0.016, 0.044), mats["glow"], head_pivot)
    cone("SpiritFlame", (0, -0.032, 2.545), 0.090, 0.260, mats["glow"], head_pivot, vertices=5, rotation=(0, 0, math.radians(18)), scale=(0.82, 0.58, 1.0))
    sphere("SpiritFlameCore", (0.010, -0.060, 2.460), (0.048, 0.022, 0.065), mats["glow"], head_pivot)
    for idx, x in enumerate([-0.115, 0.0, 0.115]):
        cone(f"CrownFanSpike{idx + 1}", (x, -0.018, 2.480 + (0.030 if x == 0 else 0)), 0.032, 0.165, mats["metal"], head_pivot, vertices=4, rotation=(0, 0, math.radians(8 if x < 0 else -8)), scale=(0.72, 0.58, 1.0))
    for idx, x in enumerate([-0.170, -0.085, 0.085, 0.170]):
        cylinder(f"CrownSidePin{idx + 1}", (x, -0.094, 2.360), 0.012, 0.220, mats["metal"], head_pivot, vertices=8, rotation=(0, math.radians(86), 0))
        sphere(f"CrownSideBead{idx + 1}", (x * 1.34, -0.104, 2.360), (0.026, 0.014, 0.026), mats["jade"], head_pivot)
    for idx, x in enumerate([-0.235, 0.235]):
        talisman(f"CrownHangingSeal{idx + 1}", (x, -0.145, 2.205), head_pivot, mats, scale=(0.058, 0.005, 0.175), glow=idx == 0)
    for idx, x in enumerate([-0.300, -0.105, 0.105, 0.300]):
        talisman(f"HeadSideSeal{idx + 1}", (x, -0.155, 2.070), head_pivot, mats, scale=(0.040, 0.004, 0.132), rotation=(0, 0, math.radians(-9 if x < 0 else 9)), glow=False)
    for idx, x in enumerate([-0.265, -0.205, 0.205, 0.265]):
        sphere(f"HeadChainBead{idx + 1}", (x, -0.158, 2.230), (0.018, 0.010, 0.018), mats["metal"], head_pivot)


def add_torso_details(spine, body, mats):
    bevel_object(cube("LayeredBreastplateTop", (0, -0.326, 1.355), (0.50, 0.030, 0.070), mats["metal"], spine), 0.012, 1)
    bevel_object(cube("LayeredBreastplateMid", (0, -0.330, 1.245), (0.42, 0.028, 0.060), mats["paper"], spine), 0.010, 1)
    bevel_object(cube("LayeredBreastplateLow", (0, -0.324, 1.130), (0.35, 0.026, 0.055), mats["metal"], spine), 0.010, 1)
    bevel_object(cube("RobeDiagonalCordA", (-0.115, -0.352, 1.350), (0.035, 0.018, 0.350), mats["metal"], spine), 0.004, 1).rotation_euler.z = math.radians(-22)
    bevel_object(cube("RobeDiagonalCordB", (0.115, -0.352, 1.350), (0.035, 0.018, 0.350), mats["metal"], spine), 0.004, 1).rotation_euler.z = math.radians(22)
    for idx, x in enumerate([-0.260, -0.170, -0.080, 0.080, 0.170, 0.260]):
        sphere(f"CollarPearl{idx + 1}", (x, -0.356, 1.495 - abs(x) * 0.13), (0.020, 0.010, 0.020), mats["jade"], spine)
    for idx, x in enumerate([-0.245, -0.125, 0.125, 0.245]):
        sphere(f"BreastplateGem{idx + 1}", (x, -0.350, 1.245), (0.018, 0.010, 0.018), mats["glow"], spine)
    talisman("BellyCenterCharm", (0, -0.332, 0.905), body, mats, scale=(0.088, 0.008, 0.245), glow=True)
    for idx, x in enumerate([-0.37, 0.37]):
        talisman(f"HipSideTalisman{idx + 1}", (x, -0.228, 0.595), body, mats, scale=(0.062, 0.007, 0.235), rotation=(0, 0, math.radians(8 if x < 0 else -8)), glow=False)
    for idx, x in enumerate([-0.205, 0.0, 0.205]):
        bevel_object(cube(f"RobeLowerLayer{idx + 1}", (x, -0.312, 0.710), (0.155, 0.024, 0.250), mats["accent" if idx == 1 else "trim"], body), 0.010, 1).rotation_euler.z = math.radians((idx - 1) * 4)
    for idx, x in enumerate([-0.285, -0.095, 0.095, 0.285]):
        sphere(f"LowerRobeBell{idx + 1}", (x, -0.330, 0.585), (0.022, 0.014, 0.026), mats["metal"], body)


def add_hand_details(label, side, elbow, mats):
    for finger_idx, finger_x in enumerate([-0.045, 0.0, 0.045]):
        bevel_object(
            cube(
                f"{label}_Finger{finger_idx + 1}",
                (side * (0.02 + finger_x), -0.170, -0.70),
                (0.028, 0.055, 0.070),
                mats["trim"],
                elbow,
            ),
            0.006,
            1,
        )
        # narrow claw tip
        bevel_object(
            cube(
                f"{label}_Claw{finger_idx + 1}",
                (side * (0.02 + finger_x), -0.212, -0.748),
                (0.020, 0.032, 0.025),
                mats["paper"],
                elbow,
                local=True,
            ),
            0.004,
            1,
        )
    for stud_idx, stud_x in enumerate([-0.075, 0.0, 0.075]):
        sphere(f"{label}_WristStud{stud_idx + 1}", (stud_x, -0.142, -0.40), (0.014, 0.008, 0.014), mats["glow"], elbow, local=True)
    bevel_object(cube(f"{label}_PalmPlate", (side * 0.048, -0.168, -0.650), (0.150, 0.030, 0.060), mats["metal"], elbow, local=True), 0.006, 1)
    bevel_object(cube(f"{label}_SleeveCuffTop", (side * 0.038, -0.138, -0.515), (0.205, 0.040, 0.045), mats["paper"], elbow, local=True), 0.006, 1)
    bevel_object(cube(f"{label}_SleeveCuffGlow", (side * 0.038, -0.166, -0.515), (0.142, 0.014, 0.018), mats["glow"], elbow, local=True), 0.003, 1)


def add_foot_details(label, knee, mats):
    for toe_idx, toe_x in enumerate([-0.115, 0, 0.115]):
        bevel_object(cube(f"{label}_ToePlate{toe_idx + 1}", (toe_x, -0.585, -0.505), (0.072, 0.040, 0.052), mats["paper"], knee, local=True), 0.006, 1)
        bevel_object(cube(f"{label}_ToeClaw{toe_idx + 1}", (toe_x, -0.655, -0.492), (0.044, 0.030, 0.034), mats["metal"], knee, local=True), 0.006, 1)
    bevel_object(cube(f"{label}_BootGlowSlash", (0, -0.520, -0.445), (0.250, 0.018, 0.020), mats["glow"], knee, local=True), 0.003, 1)


def add_weapon_details(weapon, mats):
    for idx, y in enumerate([-0.56, -0.68, -0.80, -0.92, -1.04]):
        bevel_object(cube(f"SwordRuneStrokeA{idx + 1}", (-0.030, y, 0.133), (0.012, 0.060, 0.010), mats["seal"], weapon, local=True), 0.002, 1)
        bevel_object(cube(f"SwordRuneStrokeB{idx + 1}", (0.030, y + 0.025, 0.133), (0.012, 0.048, 0.010), mats["seal"], weapon, local=True), 0.002, 1)
    for idx, y in enumerate([-0.50, -0.63, -0.76, -0.89, -1.02, -1.15]):
        sphere(f"SwordRuneGem{idx + 1}", (0, y, 0.145), (0.014, 0.005, 0.014), mats["glow"], weapon, local=True)
    bevel_object(cube("SwordInscriptionLineA", (-0.064, -0.86, 0.146), (0.010, 0.72, 0.008), mats["seal"], weapon, local=True), 0.002, 1)
    bevel_object(cube("SwordInscriptionLineB", (0.064, -0.86, 0.146), (0.010, 0.72, 0.008), mats["seal"], weapon, local=True), 0.002, 1)
