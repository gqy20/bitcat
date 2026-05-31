import bpy


def clear_scene():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete()


def make_material(name, color, emission=False, roughness=0.52, metallic=0.0):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = color
    mat.use_nodes = True
    bsdf = next((node for node in mat.node_tree.nodes if node.type == "BSDF_PRINCIPLED"), None)
    if bsdf:
        if "Base Color" in bsdf.inputs:
            bsdf.inputs["Base Color"].default_value = color
        if "Roughness" in bsdf.inputs:
            bsdf.inputs["Roughness"].default_value = roughness
        if "Metallic" in bsdf.inputs:
            bsdf.inputs["Metallic"].default_value = metallic
        if emission and "Emission Color" in bsdf.inputs:
            bsdf.inputs["Emission Color"].default_value = color
        if emission and "Emission Strength" in bsdf.inputs:
            bsdf.inputs["Emission Strength"].default_value = 0.35
    return mat


def mix_color(a, b, t):
    return (
        a[0] * (1 - t) + b[0] * t,
        a[1] * (1 - t) + b[1] * t,
        a[2] * (1 - t) + b[2] * t,
        a[3] if len(a) > 3 else 1,
    )


def empty(name, loc=(0, 0, 0), parent=None, local=False):
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = "PLAIN_AXES"
    obj.empty_display_size = 0.12
    obj.location = loc
    bpy.context.collection.objects.link(obj)
    if parent:
        obj.parent = parent
        if local:
            obj.location = loc
    return obj


def attach_parent(obj, parent, loc, local=False):
    if not parent:
        return
    if local:
        obj.parent = parent
        obj.location = loc
    else:
        set_world_parent(obj, parent)


def cube(name, loc, scale, mat, parent=None, local=False):
    bpy.ops.mesh.primitive_cube_add(size=1, location=loc)
    obj = bpy.context.object
    obj.name = name
    obj.dimensions = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    attach_parent(obj, parent, loc, local)
    return obj


def smooth_object(obj):
    for poly in obj.data.polygons:
        poly.use_smooth = True
    return obj


def bevel_object(obj, amount=0.035, segments=4):
    mod = obj.modifiers.new("soft_bevel", "BEVEL")
    mod.width = amount
    mod.segments = segments
    mod.affect = "EDGES"
    obj.modifiers.new("weighted_normals", "WEIGHTED_NORMAL")
    smooth_object(obj)
    return obj


def sphere(name, loc, scale, mat, parent=None, local=False):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=48, ring_count=24, radius=1, location=loc)
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    smooth_object(obj)
    attach_parent(obj, parent, loc, local)
    return obj


def cylinder(name, loc, radius, depth, mat, parent=None, vertices=40, rotation=(0, 0, 0), scale=(1, 1, 1), local=False):
    bpy.ops.mesh.primitive_cylinder_add(vertices=vertices, radius=radius, depth=depth, location=loc, rotation=rotation)
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    bevel_object(obj, amount=0.014, segments=3)
    attach_parent(obj, parent, loc, local)
    return obj


def capsule(name, loc, radius, depth, mat, parent=None, rotation=(0, 0, 0), scale=(1, 1, 1), local=False):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=48, ring_count=24, radius=1, location=loc, rotation=rotation)
    obj = bpy.context.object
    obj.name = name
    obj.scale = (radius * scale[0], radius * scale[1], depth * 0.5 * scale[2])
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    smooth_object(obj)
    attach_parent(obj, parent, loc, local)
    return obj


def torus(name, loc, major_radius, minor_radius, mat, parent=None, rotation=(0, 0, 0), scale=(1, 1, 1), local=False):
    bpy.ops.mesh.primitive_torus_add(
        major_segments=64,
        minor_segments=12,
        major_radius=major_radius,
        minor_radius=minor_radius,
        location=loc,
        rotation=rotation,
    )
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    smooth_object(obj)
    attach_parent(obj, parent, loc, local)
    return obj


def talisman(name, loc, parent, mats, scale=(0.12, 0.012, 0.32), rotation=(0, 0, 0), glow=False, local=False):
    tag = bevel_object(cube(f"{name}Paper", loc, scale, mats["paper"], parent, local=local), 0.004, 1)
    tag.rotation_euler = rotation
    line_y = loc[1] - 0.009
    ink = mats["glow"] if glow else mats["ink"]
    for idx, dz in enumerate([-0.08, 0.0, 0.08]):
        stroke = bevel_object(
            cube(
                f"{name}Rune{idx + 1}",
                (loc[0], line_y - 0.006, loc[2] + dz),
                (scale[0] * (0.58 if idx != 1 else 0.42), 0.006, 0.018),
                ink,
                parent,
                local=local,
            ),
            0.002,
            1,
        )
        stroke.rotation_euler = rotation
    bead = sphere(f"{name}Seal", (loc[0], line_y - 0.010, loc[2] + scale[2] * 0.38), (0.018, 0.006, 0.018), mats["seal"], parent, local=local)
    bead.rotation_euler = rotation
    return tag


def cone(name, loc, radius1, depth, mat, parent=None, vertices=3, rotation=(0, 0, 0), scale=(1, 1, 1), local=False):
    bpy.ops.mesh.primitive_cone_add(vertices=vertices, radius1=radius1, radius2=0, depth=depth, location=loc, rotation=rotation)
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    obj.data.materials.append(mat)
    bevel_object(obj, amount=0.006, segments=1)
    attach_parent(obj, parent, loc, local)
    return obj


def set_world_parent(child, parent):
    matrix = child.matrix_world.copy()
    child.parent = parent
    child.matrix_parent_inverse = parent.matrix_world.inverted()
    child.matrix_world = matrix
