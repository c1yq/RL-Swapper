"""
rl_color_injector.py
~~~~~~~~~~~~~~~~~~~~
Direct binary color & paint variant programming for Rocket League cooked UPK packages.

Programs any of the 14 official Rocket League painted variants or any custom #RRGGBB
color directly into decrypted UObject material, mesh-skin, and particle properties,
and neutralizes ProductAttribute_PaintSettings_TA so server-side sacrifice item paint
attributes never override the programmed color.
"""

from __future__ import annotations

import re
import struct
from dataclasses import dataclass
from typing import Dict, List, Optional, Sequence, Set, Tuple

import rl_upk_editor as upk


@dataclass(frozen=True)
class ColorSpec:
    """Represents either an official RL paint preset or a custom #RRGGBB color."""
    name: str
    hex_srgb: str
    linear_rgb: Tuple[float, float, float]
    is_custom: bool = False

    @property
    def label(self) -> str:
        if self.is_custom:
            return f"Custom {self.hex_srgb.upper()}"
        return self.name


# Official 14 Rocket League Paint Colors (calibrated for UE3 LinearColor shaders + UI hex swatches)
OFFICIAL_PAINTS: Tuple[ColorSpec, ...] = (
    ColorSpec("Titanium White", "#F5F5F7", (0.8800, 0.8800, 0.8850)),
    ColorSpec("Black",          "#1C1C1F", (0.0120, 0.0120, 0.0125)),
    ColorSpec("Gold",           "#E5B842", (0.7600, 0.4600, 0.0550)),
    ColorSpec("Crimson",        "#E02424", (0.6800, 0.0180, 0.0180)),
    ColorSpec("Sky Blue",       "#29B6F6", (0.0350, 0.4500, 0.8800)),
    ColorSpec("Cobalt",         "#2558E8", (0.0250, 0.1000, 0.7200)),
    ColorSpec("Purple",         "#8E24AA", (0.2600, 0.0220, 0.5800)),
    ColorSpec("Pink",           "#F472B6", (0.8500, 0.1600, 0.3800)),
    ColorSpec("Forest Green",   "#1B8A3D", (0.0180, 0.3000, 0.0350)),
    ColorSpec("Lime",           "#84E119", (0.2400, 0.8200, 0.0150)),
    ColorSpec("Saffron",        "#FACC15", (0.8800, 0.6200, 0.0150)),
    ColorSpec("Orange",         "#F97316", (0.8800, 0.2000, 0.0080)),
    ColorSpec("Burnt Sienna",   "#7C3A16", (0.1800, 0.0320, 0.0060)),
    ColorSpec("Grey",           "#7A7F87", (0.1900, 0.1950, 0.2050)),
)

PAINT_BY_NAME: Dict[str, ColorSpec] = {p.name.lower(): p for p in OFFICIAL_PAINTS}

# Slots where visual 3D/material/particle color programming applies
COLORABLE_SLOTS: Set[str] = {
    "Body",
    "Decal",
    "Wheels",
    "Rocket Boost",
    "Goal Explosion",
    "Trail",
    "Topper",
    "Antenna",
}

_HEX_RE = re.compile(r"^#?([0-9a-fA-F]{6})$")


def srgb_hex_to_linear(hex_code: str) -> Tuple[str, Tuple[float, float, float]]:
    m = _HEX_RE.match((hex_code or "").strip())
    if not m:
        raise ValueError(f"Invalid #RRGGBB color code: {hex_code!r}")
    h = m.group(1).upper()
    r8 = int(h[0:2], 16)
    g8 = int(h[2:4], 16)
    b8 = int(h[4:6], 16)
    # UE3 FLinearColor(const FColor&) uses pow(c / 255.0, 2.2)
    return f"#{h}", (
        round((r8 / 255.0) ** 2.2, 6),
        round((g8 / 255.0) ** 2.2, 6),
        round((b8 / 255.0) ** 2.2, 6),
    )


def resolve_color_spec(value: Optional[str]) -> Optional[ColorSpec]:
    """Parse None/'default', an official RL paint name, or a '#RRGGBB' string into a ColorSpec."""
    if not value:
        return None
    cleaned = value.strip()
    if not cleaned or cleaned.lower() in ("none", "default", "original", "unpainted"):
        return None
    preset = PAINT_BY_NAME.get(cleaned.lower())
    if preset is not None:
        return preset
    norm_hex, lin = srgb_hex_to_linear(cleaned)
    return ColorSpec(name=norm_hex, hex_srgb=norm_hex, linear_rgb=lin, is_custom=True)


# ── Low-Level UE3 Property Tag Helpers ───────────────────────────────────────

def _pack_fname(pkg: upk.ParsedPackage, text: str) -> bytes:
    return upk._pack_fname_value(pkg, text)


def _build_name_prop(pkg: upk.ParsedPackage, prop_name: str, val_name: str) -> bytes:
    val_bytes = _pack_fname(pkg, val_name)
    return (
        _pack_fname(pkg, prop_name)
        + _pack_fname(pkg, "NameProperty")
        + struct.pack("<ii", len(val_bytes), 0)
        + val_bytes
    )


def _build_linear_color_prop(
    pkg: upk.ParsedPackage,
    prop_name: str,
    rgba: Tuple[float, float, float, float],
) -> bytes:
    val_bytes = struct.pack("<ffff", *rgba)
    return (
        _pack_fname(pkg, prop_name)
        + _pack_fname(pkg, "StructProperty")
        + struct.pack("<ii", 16, 0)
        + _pack_fname(pkg, "LinearColor")
        + val_bytes
    )


def _build_guid_prop(pkg: upk.ParsedPackage, prop_name: str, guid_bytes: bytes = b"\x00" * 16) -> bytes:
    return (
        _pack_fname(pkg, prop_name)
        + _pack_fname(pkg, "StructProperty")
        + struct.pack("<ii", 16, 0)
        + _pack_fname(pkg, "Guid")
        + guid_bytes[:16].ljust(16, b"\x00")
    )


def _build_mic_vector_param_elem(
    pkg: upk.ParsedPackage,
    param_name: str,
    rgba: Tuple[float, float, float, float],
    guid_bytes: bytes = b"\x00" * 16,
) -> bytes:
    """136-byte FVectorParameterValue element for MaterialInstanceConstant.VectorParameterValues."""
    return (
        _build_name_prop(pkg, "ParameterName", param_name)
        + _build_linear_color_prop(pkg, "ParameterValue", rgba)
        + _build_guid_prop(pkg, "ExpressionGUID", guid_bytes)
        + _pack_fname(pkg, "None")
    )


def _build_skin_vector_param_elem(
    pkg: upk.ParsedPackage,
    param_name: str,
    rgba: Tuple[float, float, float, float],
) -> bytes:
    """88-byte FMaterialParamVector element for ProductAsset_Body_TA.SkinParameters.VectorParameters."""
    return (
        _build_name_prop(pkg, "Name", param_name)
        + _build_linear_color_prop(pkg, "Value", rgba)
        + _pack_fname(pkg, "None")
    )


def _patch_bytes_inplace(pkg: upk.ParsedPackage, abs_offset: int, payload: bytes) -> upk.ParsedPackage:
    buf = bytearray(pkg.file_bytes)
    buf[abs_offset:abs_offset + len(payload)] = payload
    return upk.ParsedPackage(
        file_path=pkg.file_path,
        summary=pkg.summary,
        names=pkg.names,
        imports=pkg.imports,
        exports=pkg.exports,
        file_bytes=bytes(buf),
    )


def _find_donor_product_exports(pkg: upk.ParsedPackage, donor_asset_path: Optional[str]) -> List[upk.ExportEntry]:
    if donor_asset_path:
        idx = upk.resolve_object_index_by_text(pkg, donor_asset_path)
        if idx is not None and idx > 0:
            return [pkg.exports[idx - 1]]
        leaf = donor_asset_path.split(".")[-1].lower()
        matches = [
            e for e in pkg.exports
            if pkg.export_class_name(e).startswith("ProductAsset_")
            and pkg.resolve_name(e.object_name).lower() == leaf
        ]
        if matches:
            return matches
    return [e for e in pkg.exports if pkg.export_class_name(e).startswith("ProductAsset_")]


def _parse_paint_settings_info(
    pkg: upk.ParsedPackage,
    donor_pa_indices: Set[int],
) -> Tuple[Optional[str], List[int], Set[int]]:
    """
    Inspect ProductAttribute_PaintSettings_TA exports.
    Returns (paint_param_name, material_group_export_indices, paint_settings_export_indices_1based).
    """
    paint_param: Optional[str] = None
    mat_indices: List[int] = []
    ps_indices: Set[int] = set()

    # Collect PaintSettings referenced via Attributes on donor ProductAsset exports
    referenced_ps: Set[int] = set()
    for pa_idx1 in donor_pa_indices:
        pa_exp = pkg.exports[pa_idx1 - 1]
        pa_raw = pkg.object_data(pa_exp)
        for prop in upk.parse_serialized_properties(pkg, pa_exp, None):
            if prop.name == "Attributes" and prop.tag_type == "ArrayProperty" and prop.size >= 4:
                vb = pa_raw[prop.value_offset:prop.value_offset + prop.size]
                cnt = struct.unpack_from("<i", vb, 0)[0]
                for i in range(max(0, min(cnt, (len(vb) - 4) // 4))):
                    ref = struct.unpack_from("<i", vb, 4 + i * 4)[0]
                    if ref > 0:
                        referenced_ps.add(ref)

    all_ps = [
        e for e in pkg.exports
        if "PaintSettings" in pkg.export_class_name(e)
    ]
    target_ps = [
        e for e in all_ps
        if (e.table_index + 1) in referenced_ps or e.outer_index in donor_pa_indices
    ] or all_ps

    for exp in target_ps:
        ps_indices.add(exp.table_index + 1)
        raw = pkg.object_data(exp)
        for p in upk.parse_serialized_properties(pkg, exp, None):
            if p.name == "PaintParameterName" and p.tag_type == "NameProperty":
                if p.value and p.value != "None":
                    paint_param = p.value
            elif p.name == "MaterialGroups" and p.tag_type == "StructProperty":
                sb = raw[p.value_offset:p.value_offset + p.size]
                _, sprops, _ = upk._try_parse_property_stream(pkg, sb, 0)
                for sp in sprops:
                    if sp.name == "Materials" and sp.tag_type == "ArrayProperty" and sp.size >= 4:
                        ab = sb[sp.value_offset:sp.value_offset + sp.size]
                        cnt = struct.unpack_from("<i", ab, 0)[0]
                        for i in range(max(0, min(cnt, (len(ab) - 4) // 4))):
                            midx = struct.unpack_from("<i", ab, 4 + i * 4)[0]
                            if midx > 0 and midx not in mat_indices:
                                mat_indices.append(midx)

    return paint_param, mat_indices, ps_indices


def _neutralize_paint_attributes(pkg: upk.ParsedPackage, donor_pa_indices: Set[int]) -> Tuple[upk.ParsedPackage, int]:
    """Zero out ProductAttribute_PaintSettings_TA references inside ProductAsset.Attributes in-place."""
    cleared = 0
    for pa_idx1 in donor_pa_indices:
        pa_exp = pkg.exports[pa_idx1 - 1]
        pa_raw = pkg.object_data(pa_exp)
        for prop in upk.parse_serialized_properties(pkg, pa_exp, None):
            if prop.name == "Attributes" and prop.tag_type == "ArrayProperty" and prop.size >= 4:
                vb = pa_raw[prop.value_offset:prop.value_offset + prop.size]
                cnt = struct.unpack_from("<i", vb, 0)[0]
                for i in range(max(0, min(cnt, (len(vb) - 4) // 4))):
                    elem_rel = prop.value_offset + 4 + i * 4
                    ref = struct.unpack_from("<i", pa_raw, elem_rel)[0]
                    if ref > 0 and ref <= len(pkg.exports):
                        ref_cls = pkg.export_class_name(pkg.exports[ref - 1])
                        if "PaintSettings" in ref_cls:
                            pkg = _patch_bytes_inplace(pkg, pa_exp.serial_offset + elem_rel, struct.pack("<i", 0))
                            pa_raw = pkg.object_data(pkg.exports[pa_idx1 - 1])
                            cleared += 1
    return pkg, cleared


def _upsert_mic_vector_param(
    pkg: upk.ParsedPackage,
    mic_table_idx: int,
    target_names_lower: Set[str],
    insert_param_name: Optional[str],
    rgb: Tuple[float, float, float],
    force_alpha: Optional[float] = None,
) -> Tuple[upk.ParsedPackage, int]:
    """
    Update matching VectorParameterValues in a MaterialInstanceConstant in-place (preserving alpha
    unless force_alpha is set). If none matched and insert_param_name is given, append/create the parameter.
    """
    exp = pkg.exports[mic_table_idx]
    raw = pkg.object_data(exp)
    if not raw:
        return pkg, 0

    prop_start, props = upk._find_best_property_stream_offset(pkg, raw, None, None)
    end_off, _, ended = upk._try_parse_property_stream(pkg, raw, prop_start)

    vec_prop: Optional[upk.ParsedProperty] = None
    for p in props:
        if p.name == "VectorParameterValues" and p.tag_type == "ArrayProperty":
            vec_prop = p
            break

    patched_count = 0
    if vec_prop is not None and vec_prop.size >= 4:
        vb = raw[vec_prop.value_offset:vec_prop.value_offset + vec_prop.size]
        cnt = struct.unpack_from("<i", vb, 0)[0]
        pos = 4
        for _ in range(max(0, min(cnt, 64))):
            next_pos, eprops, e_ended = upk._try_parse_property_stream(pkg, vb, pos)
            if not e_ended and not eprops:
                break
            pname_prop = next((ep for ep in eprops if ep.name == "ParameterName" and ep.tag_type == "NameProperty"), None)
            pval_prop = next((ep for ep in eprops if ep.name == "ParameterValue" and ep.size == 16), None)
            if pname_prop and pval_prop and pname_prop.value.lower() in target_names_lower:
                val_rel_in_exp = vec_prop.value_offset + pval_prop.value_offset
                orig_rgba = struct.unpack_from("<ffff", raw, val_rel_in_exp)
                alpha = force_alpha if force_alpha is not None else orig_rgba[3]
                new_rgba_bytes = struct.pack("<ffff", rgb[0], rgb[1], rgb[2], alpha)
                pkg = _patch_bytes_inplace(pkg, exp.serial_offset + val_rel_in_exp, new_rgba_bytes)
                raw = pkg.object_data(pkg.exports[mic_table_idx])
                vb = raw[vec_prop.value_offset:vec_prop.value_offset + vec_prop.size]
                patched_count += 1
            pos = next_pos

        if patched_count == 0 and insert_param_name:
            # Append a new 136-byte element to existing VectorParameterValues TArray
            pkg = upk.ensure_package_names(pkg, [
                insert_param_name, "ParameterName", "NameProperty",
                "ParameterValue", "StructProperty", "LinearColor",
                "ExpressionGUID", "Guid", "None",
            ])
            exp = pkg.exports[mic_table_idx]
            raw = pkg.object_data(exp)
            _, props = upk._find_best_property_stream_offset(pkg, raw, None, None)
            vec_prop = next(p for p in props if p.name == "VectorParameterValues")
            elem_bytes = _build_mic_vector_param_elem(
                pkg, insert_param_name, (rgb[0], rgb[1], rgb[2], force_alpha if force_alpha is not None else 1.0)
            )
            old_vb = raw[vec_prop.value_offset:vec_prop.value_offset + vec_prop.size]
            old_cnt = struct.unpack_from("<i", old_vb, 0)[0]
            new_vb = struct.pack("<i", old_cnt + 1) + old_vb[4:] + elem_bytes
            new_raw = bytearray()
            new_raw += raw[:vec_prop.tag_offset + 16]
            new_raw += struct.pack("<i", len(new_vb))
            new_raw += raw[vec_prop.tag_offset + 20:vec_prop.value_offset]
            new_raw += new_vb
            new_raw += raw[vec_prop.value_offset + vec_prop.size:]
            pkg = upk.replace_export_data(pkg, exp, bytes(new_raw))
            return pkg, 1

        return pkg, patched_count

    # VectorParameterValues property does not exist yet on this MIC; insert before None terminator
    if insert_param_name and ended and end_off >= 8:
        pkg = upk.ensure_package_names(pkg, [
            "VectorParameterValues", "ArrayProperty",
            insert_param_name, "ParameterName", "NameProperty",
            "ParameterValue", "StructProperty", "LinearColor",
            "ExpressionGUID", "Guid", "None",
        ])
        exp = pkg.exports[mic_table_idx]
        raw = pkg.object_data(exp)
        prop_start, _ = upk._find_best_property_stream_offset(pkg, raw, None, None)
        end_off, _, ended = upk._try_parse_property_stream(pkg, raw, prop_start)
        if ended and end_off >= 8:
            elem_bytes = _build_mic_vector_param_elem(
                pkg, insert_param_name, (rgb[0], rgb[1], rgb[2], force_alpha if force_alpha is not None else 1.0)
            )
            arr_val = struct.pack("<i", 1) + elem_bytes
            arr_prop = (
                _pack_fname(pkg, "VectorParameterValues")
                + _pack_fname(pkg, "ArrayProperty")
                + struct.pack("<ii", len(arr_val), 0)
                + arr_val
            )
            none_pos = end_off - 8
            new_raw = raw[:none_pos] + arr_prop + raw[none_pos:]
            pkg = upk.replace_export_data(pkg, exp, new_raw)
            return pkg, 1

    return pkg, 0


def _upsert_body_skin_parameters(
    pkg: upk.ParsedPackage,
    pa_table_idx: int,
    param_name: str,
    rgb: Tuple[float, float, float],
) -> Tuple[upk.ParsedPackage, int]:
    """Update or insert param_name (e.g. TrimColor) inside ProductAsset_Body_TA.SkinParameters.VectorParameters."""
    pkg = upk.ensure_package_names(pkg, [
        "SkinParameters", "MaterialParams", "VectorParameters", "ArrayProperty",
        "Name", "NameProperty", "Value", "StructProperty", "LinearColor",
        param_name, "None",
    ])
    exp = pkg.exports[pa_table_idx]
    raw = pkg.object_data(exp)
    if not raw:
        return pkg, 0

    prop_start, props = upk._find_best_property_stream_offset(pkg, raw, None, None)
    end_off, _, ended = upk._try_parse_property_stream(pkg, raw, prop_start)
    skin_prop = next((p for p in props if p.name == "SkinParameters" and p.tag_type == "StructProperty"), None)

    rgba = (rgb[0], rgb[1], rgb[2], 1.0)

    if skin_prop is not None:
        sb = raw[skin_prop.value_offset:skin_prop.value_offset + skin_prop.size]
        s_end, sprops, s_ended = upk._try_parse_property_stream(pkg, sb, 0)
        vparams_prop = next((sp for sp in sprops if sp.name == "VectorParameters" and sp.tag_type == "ArrayProperty"), None)

        if vparams_prop is not None and vparams_prop.size >= 4:
            vb = sb[vparams_prop.value_offset:vparams_prop.value_offset + vparams_prop.size]
            cnt = struct.unpack_from("<i", vb, 0)[0]
            pos = 4
            for _ in range(max(0, min(cnt, 64))):
                next_pos, eprops, e_ended = upk._try_parse_property_stream(pkg, vb, pos)
                if not e_ended and not eprops:
                    break
                n_prop = next((ep for ep in eprops if ep.name == "Name" and ep.tag_type == "NameProperty"), None)
                v_prop = next((ep for ep in eprops if ep.name == "Value" and ep.size == 16), None)
                if n_prop and v_prop and n_prop.value.lower() == param_name.lower():
                    abs_off = exp.serial_offset + skin_prop.value_offset + vparams_prop.value_offset + v_prop.value_offset
                    pkg = _patch_bytes_inplace(pkg, abs_off, struct.pack("<ffff", *rgba))
                    return pkg, 1
                pos = next_pos

            # Append 88-byte element to VectorParameters inside SkinParameters
            elem_bytes = _build_skin_vector_param_elem(pkg, param_name, rgba)
            new_vb = struct.pack("<i", cnt + 1) + vb[4:] + elem_bytes
            new_sb = bytearray()
            new_sb += sb[:vparams_prop.tag_offset + 16]
            new_sb += struct.pack("<i", len(new_vb))
            new_sb += sb[vparams_prop.tag_offset + 20:vparams_prop.value_offset]
            new_sb += new_vb
            new_sb += sb[vparams_prop.value_offset + vparams_prop.size:]

            new_raw = bytearray()
            new_raw += raw[:skin_prop.tag_offset + 16]
            new_raw += struct.pack("<i", len(new_sb))
            new_raw += raw[skin_prop.tag_offset + 20:skin_prop.value_offset]
            new_raw += new_sb
            new_raw += raw[skin_prop.value_offset + skin_prop.size:]
            pkg = upk.replace_export_data(pkg, exp, bytes(new_raw))
            return pkg, 1

        if s_ended and s_end >= 8:
            elem_bytes = _build_skin_vector_param_elem(pkg, param_name, rgba)
            arr_val = struct.pack("<i", 1) + elem_bytes
            arr_prop = (
                _pack_fname(pkg, "VectorParameters")
                + _pack_fname(pkg, "ArrayProperty")
                + struct.pack("<ii", len(arr_val), 0)
                + arr_val
            )
            none_pos = s_end - 8
            new_sb = sb[:none_pos] + arr_prop + sb[none_pos:]
            new_raw = bytearray()
            new_raw += raw[:skin_prop.tag_offset + 16]
            new_raw += struct.pack("<i", len(new_sb))
            new_raw += raw[skin_prop.tag_offset + 20:skin_prop.value_offset]
            new_raw += new_sb
            new_raw += raw[skin_prop.value_offset + skin_prop.size:]
            pkg = upk.replace_export_data(pkg, exp, bytes(new_raw))
            return pkg, 1

    # SkinParameters does not exist on ProductAsset_Body_TA at all; insert whole struct before None
    if ended and end_off >= 8:
        elem_bytes = _build_skin_vector_param_elem(pkg, param_name, rgba)
        arr_val = struct.pack("<i", 1) + elem_bytes
        arr_prop = (
            _pack_fname(pkg, "VectorParameters")
            + _pack_fname(pkg, "ArrayProperty")
            + struct.pack("<ii", len(arr_val), 0)
            + arr_val
        )
        struct_val = arr_prop + _pack_fname(pkg, "None")
        skin_tag = (
            _pack_fname(pkg, "SkinParameters")
            + _pack_fname(pkg, "StructProperty")
            + struct.pack("<ii", len(struct_val), 0)
            + _pack_fname(pkg, "MaterialParams")
            + struct_val
        )
        none_pos = end_off - 8
        new_raw = raw[:none_pos] + skin_tag + raw[none_pos:]
        pkg = upk.replace_export_data(pkg, exp, new_raw)
        return pkg, 1

    return pkg, 0


def _patch_material_expression_vector_params(
    pkg: upk.ParsedPackage,
    target_names_lower: Set[str],
    rgb: Tuple[float, float, float],
) -> Tuple[upk.ParsedPackage, int]:
    """Patch MaterialExpressionVectorParameter.DefaultValue (LinearColor, 16B) in-place."""
    count = 0
    for idx, exp in enumerate(pkg.exports):
        if pkg.export_class_name(exp) != "MaterialExpressionVectorParameter":
            continue
        raw = pkg.object_data(exp)
        props = upk.parse_serialized_properties(pkg, exp, None)
        pname_prop = next((p for p in props if p.name == "ParameterName" and p.tag_type == "NameProperty"), None)
        dval_prop = next((p for p in props if p.name == "DefaultValue" and p.size == 16), None)
        if not pname_prop or pname_prop.value.lower() not in target_names_lower:
            continue
        if dval_prop is not None:
            orig_rgba = struct.unpack_from("<ffff", raw, dval_prop.value_offset)
            new_bytes = struct.pack("<ffff", rgb[0], rgb[1], rgb[2], orig_rgba[3])
            pkg = _patch_bytes_inplace(pkg, exp.serial_offset + dval_prop.value_offset, new_bytes)
            count += 1
        else:
            # In UE3, if DefaultValue was default (0,0,0,0) and omitted, insert DefaultValue before None
            pkg = upk.ensure_package_names(pkg, ["DefaultValue", "StructProperty", "LinearColor"])
            exp = pkg.exports[idx]
            raw = pkg.object_data(exp)
            pstart, _ = upk._find_best_property_stream_offset(pkg, raw, None, None)
            end_off, _, ended = upk._try_parse_property_stream(pkg, raw, pstart)
            if ended and end_off >= 8:
                dv_tag = _build_linear_color_prop(pkg, "DefaultValue", (rgb[0], rgb[1], rgb[2], 1.0))
                none_pos = end_off - 8
                pkg = upk.replace_export_data(pkg, exp, raw[:none_pos] + dv_tag + raw[none_pos:])
                count += 1
    return pkg, count


def _patch_particle_vector_params(
    pkg: upk.ParsedPackage,
    target_names_lower: Set[str],
    rgb: Tuple[float, float, float],
) -> Tuple[upk.ParsedPackage, int]:
    """
    Patch DistributionVectorParticleParameter (and Constant fallback) exports in-place.
    Preserves native UE3 HDR bloom peak M = clamp(max(orig_rgb), 1.0..8.0) so bright colors
    bloom authentically while Black / #000000 stays crisp dark.
    """
    count = 0
    for idx, exp in enumerate(pkg.exports):
        cls = pkg.export_class_name(exp)
        if "DistributionVectorParticleParameter" not in cls:
            continue
        raw = pkg.object_data(exp)
        props = upk.parse_serialized_properties(pkg, exp, None)
        pname_prop = next((p for p in props if p.name == "ParameterName" and p.tag_type == "NameProperty"), None)
        if pname_prop and pname_prop.value.lower() not in target_names_lower:
            # Avoid non-color particle vectors like Velocity / Size / Location
            p_low = pname_prop.value.lower()
            if "color" not in p_low and "tint" not in p_low:
                continue

        const_prop = next((p for p in props if p.name == "Constant" and p.size == 12), None)
        # Also check MinOutput / MaxOutput if present so parameter clamping never clips custom colors
        for bound_name in ("MinOutput", "MaxOutput"):
            b_prop = next((p for p in props if p.name == bound_name and p.size == 12), None)
            if b_prop is not None:
                orig_b = struct.unpack_from("<fff", raw, b_prop.value_offset)
                b_peak = min(max(orig_b[0], orig_b[1], orig_b[2], 1.0), 8.0)
                scaled_b = (rgb[0] * b_peak, rgb[1] * b_peak, rgb[2] * b_peak)
                pkg = _patch_bytes_inplace(pkg, exp.serial_offset + b_prop.value_offset, struct.pack("<fff", *scaled_b))
                raw = pkg.object_data(pkg.exports[idx])

        if const_prop is not None:
            orig_xyz = struct.unpack_from("<fff", raw, const_prop.value_offset)
            hdr_peak = min(max(orig_xyz[0], orig_xyz[1], orig_xyz[2], 1.0), 8.0)
            new_xyz = (rgb[0] * hdr_peak, rgb[1] * hdr_peak, rgb[2] * hdr_peak)
            pkg = _patch_bytes_inplace(pkg, exp.serial_offset + const_prop.value_offset, struct.pack("<fff", *new_xyz))
            count += 1
        else:
            # Constant omitted because default (0,0,0) -> insert Constant Vector(r,g,b) before None
            pkg = upk.ensure_package_names(pkg, ["Constant", "StructProperty", "Vector"])
            exp = pkg.exports[idx]
            raw = pkg.object_data(exp)
            pstart, _ = upk._find_best_property_stream_offset(pkg, raw, None, None)
            end_off, _, ended = upk._try_parse_property_stream(pkg, raw, pstart)
            if ended and end_off >= 8:
                vec_tag = (
                    _pack_fname(pkg, "Constant")
                    + _pack_fname(pkg, "StructProperty")
                    + struct.pack("<ii", 12, 0)
                    + _pack_fname(pkg, "Vector")
                    + struct.pack("<fff", rgb[0], rgb[1], rgb[2])
                )
                none_pos = end_off - 8
                pkg = upk.replace_export_data(pkg, exp, raw[:none_pos] + vec_tag + raw[none_pos:])
                count += 1
    return pkg, count


def _switch_fx_to_painted_particle_systems(pkg: upk.ParsedPackage) -> Tuple[upk.ParsedPackage, int]:
    """
    In boosts/FX like Boost_Standard_SF.upk, FXActor's ParticleSystemComponent.Template
    points to unpainted 'Drive_PS' while a sibling 'Boost_Painted_PS' / '*_Painted_PS' exists.
    Switch Template (4B ObjectProperty) to the painted ParticleSystem if present.
    """
    painted_ps_indices = [
        e.table_index + 1
        for e in pkg.exports
        if pkg.export_class_name(e) == "ParticleSystem"
        and "painted" in pkg.resolve_name(e.object_name).lower()
    ]
    if not painted_ps_indices:
        return pkg, 0

    best_painted_ps = painted_ps_indices[0]
    switched = 0
    for exp in pkg.exports:
        if "ParticleSystemComponent" not in pkg.export_class_name(exp):
            continue
        raw = pkg.object_data(exp)
        for p in upk.parse_serialized_properties(pkg, exp, None):
            if p.name == "Template" and p.tag_type == "ObjectProperty" and p.size == 4:
                cur_ref = struct.unpack_from("<i", raw, p.value_offset)[0]
                if cur_ref > 0 and cur_ref != best_painted_ps:
                    cur_name = pkg.resolve_name(pkg.exports[cur_ref - 1].object_name).lower()
                    if "drive" in cur_name or "boost" in cur_name or "trail" in cur_name or "explosion" in cur_name:
                        pkg = _patch_bytes_inplace(
                            pkg, exp.serial_offset + p.value_offset, struct.pack("<i", best_painted_ps)
                        )
                        switched += 1
    return pkg, switched


def _strip_painted_suffix(name: str) -> str:
    s = name.lower()
    for token in ("_painted", "_paintable", "_color", "_standard", "_default"):
        s = s.replace(token, "")
    return s.strip("_")


def _promote_wheel_painted_mics(
    pkg: upk.ParsedPackage,
    painted_mic_indices_1based: Sequence[int],
) -> Tuple[upk.ParsedPackage, int]:
    """
    In Rocket League wheels (e.g. Dieci, Tunica, Veloce, Stern, Sunburst, Apex, Astro-CSX),
    the StaticMesh/SkeletalMesh references the unpainted MIC export while PaintSettings.MaterialGroups
    points to a sibling *_PAINTED MIC export that has the UseCustomColor static shader switch compiled.
    Copy the painted MIC's property+shader body onto its paired unpainted MIC export.
    """
    all_mics = [e for e in pkg.exports if pkg.export_class_name(e) == "MaterialInstanceConstant"]
    if len(all_mics) < 2:
        return pkg, 0

    candidates_1based = list(painted_mic_indices_1based)
    if not candidates_1based:
        candidates_1based = [
            e.table_index + 1
            for e in all_mics
            if any(tok in pkg.resolve_name(e.object_name).lower() for tok in ("painted", "paintable", "_color"))
        ]

    promoted = 0
    used_unpainted: Set[int] = set()

    for p_idx1 in candidates_1based:
        if not (1 <= p_idx1 <= len(pkg.exports)):
            continue
        p_exp = pkg.exports[p_idx1 - 1]
        if pkg.export_class_name(p_exp) != "MaterialInstanceConstant":
            continue
        p_name = pkg.resolve_name(p_exp.object_name)
        if not any(tok in p_name.lower() for tok in ("painted", "paintable", "_color")):
            continue

        p_stem = _strip_painted_suffix(p_name)
        p_props = upk.parse_serialized_properties(pkg, p_exp, None)
        p_parent = next((pr.value for pr in p_props if pr.name == "Parent"), None)

        best_match: Optional[upk.ExportEntry] = None
        for other in all_mics:
            o_idx1 = other.table_index + 1
            if o_idx1 == p_idx1 or o_idx1 in candidates_1based or o_idx1 in used_unpainted:
                continue
            o_name = pkg.resolve_name(other.object_name)
            if any(tok in o_name.lower() for tok in ("painted", "paintable", "_color", "tire_")):
                continue
            if _strip_painted_suffix(o_name) == p_stem:
                best_match = other
                break
            if p_parent is not None:
                o_props = upk.parse_serialized_properties(pkg, other, None)
                o_parent = next((pr.value for pr in o_props if pr.name == "Parent"), None)
                if o_parent == p_parent and best_match is None:
                    best_match = other

        if best_match is not None:
            used_unpainted.add(best_match.table_index + 1)
            # Re-fetch fresh ExportEntry refs in case a prior iteration shifted offsets
            p_exp_cur = pkg.exports[p_idx1 - 1]
            u_exp_cur = pkg.exports[best_match.table_index]
            p_raw = pkg.object_data(p_exp_cur)
            u_raw = pkg.object_data(u_exp_cur)
            p_start, _ = upk._find_best_property_stream_offset(pkg, p_raw, None, None)
            u_start, _ = upk._find_best_property_stream_offset(pkg, u_raw, None, None)
            # Preserve unpainted MIC's leading NetIndex bytes + painted MIC's properties & static shader map
            merged_raw = u_raw[:u_start] + p_raw[p_start:]
            pkg = upk.replace_export_data(pkg, u_exp_cur, merged_raw)
            promoted += 1

    return pkg, promoted


# ── Public Entry Point ───────────────────────────────────────────────────────

def apply_color_to_package(
    pkg: upk.ParsedPackage,
    donor_asset_path: Optional[str],
    slot: str,
    color: ColorSpec,
) -> Tuple[upk.ParsedPackage, List[str]]:
    """
    Apply *color* across all relevant UObjects in *pkg* for item category *slot*.
    Returns (modified_package, diagnostic_log_lines).
    """
    log: List[str] = []
    rgb = color.linear_rgb
    log.append(f"Color spec: {color.label} -> LinearRGB({rgb[0]:.4f}, {rgb[1]:.4f}, {rgb[2]:.4f})")

    donor_pa_exps = _find_donor_product_exports(pkg, donor_asset_path)
    donor_pa_indices = {e.table_index + 1 for e in donor_pa_exps}

    paint_param, mat_group_indices, _ = _parse_paint_settings_info(pkg, donor_pa_indices)
    if paint_param:
        log.append(f"Detected PaintParameterName={paint_param!r}, MaterialGroups={mat_group_indices}")

    total_edits = 0
    norm_slot = "Avatar Border" if slot == "PlayerAvatarBorder" else slot

    # 1. Car Bodies
    if norm_slot == "Body":
        body_param = paint_param or "TrimColor"
        target_names = {body_param.lower(), "trimcolor"}

        for pa_idx1 in sorted(donor_pa_indices):
            if pkg.export_class_name(pkg.exports[pa_idx1 - 1]) == "ProductAsset_Body_TA":
                pkg, n = _upsert_body_skin_parameters(pkg, pa_idx1 - 1, body_param, rgb)
                total_edits += n
                if n:
                    log.append(f"Patched SkinParameters.VectorParameters[{body_param}] on Export #{pa_idx1}")

        # Patch/insert TrimColor on MaterialGroups MICs (e.g. Dominus trim/chassis) and body MICs
        mic_indices_to_insert: Set[int] = set(mat_group_indices)
        for e in pkg.exports:
            if pkg.export_class_name(e) != "MaterialInstanceConstant":
                continue
            ename = pkg.resolve_name(e.object_name).lower()
            if "body" in ename or "trim" in ename:
                mic_indices_to_insert.add(e.table_index + 1)

        for idx, exp in enumerate(list(pkg.exports)):
            if pkg.export_class_name(exp) != "MaterialInstanceConstant":
                continue
            idx1 = idx + 1
            ins_name = body_param if idx1 in mic_indices_to_insert else None
            pkg, n = _upsert_mic_vector_param(pkg, idx, target_names, ins_name, rgb, force_alpha=1.0)
            total_edits += n

        pkg, n = _patch_material_expression_vector_params(pkg, target_names, rgb)
        total_edits += n

    # 2. Decals
    elif norm_slot == "Decal":
        decal_param = paint_param or "PaintColor"
        target_names = {decal_param.lower(), "paintcolor", "customcolor"}

        # Repoint ProductAsset_Skin_TA.Skin -> painted MIC from MaterialGroups (or *_Painted MIC)
        painted_mic_idx1: Optional[int] = None
        for midx in mat_group_indices:
            if 1 <= midx <= len(pkg.exports) and pkg.export_class_name(pkg.exports[midx - 1]) == "MaterialInstanceConstant":
                painted_mic_idx1 = midx
                break
        if painted_mic_idx1 is None:
            for e in pkg.exports:
                if pkg.export_class_name(e) == "MaterialInstanceConstant" and any(
                    t in pkg.resolve_name(e.object_name).lower() for t in ("painted", "paintable")
                ):
                    painted_mic_idx1 = e.table_index + 1
                    break

        if painted_mic_idx1 is not None:
            for pa_idx1 in sorted(donor_pa_indices):
                pa_exp = pkg.exports[pa_idx1 - 1]
                pa_raw = pkg.object_data(pa_exp)
                for p in upk.parse_serialized_properties(pkg, pa_exp, None):
                    if p.name == "Skin" and p.tag_type == "ObjectProperty" and p.size == 4:
                        cur_skin = struct.unpack_from("<i", pa_raw, p.value_offset)[0]
                        if cur_skin != painted_mic_idx1:
                            pkg = _patch_bytes_inplace(
                                pkg, pa_exp.serial_offset + p.value_offset, struct.pack("<i", painted_mic_idx1)
                            )
                            log.append(f"Switched ProductAsset_Skin_TA.Skin #{cur_skin} -> Painted MIC #{painted_mic_idx1}")
                            total_edits += 1

        for idx, exp in enumerate(list(pkg.exports)):
            if pkg.export_class_name(exp) != "MaterialInstanceConstant":
                continue
            idx1 = idx + 1
            ins_name = decal_param if (idx1 == painted_mic_idx1 or idx1 in mat_group_indices) else None
            pkg, n = _upsert_mic_vector_param(pkg, idx, target_names, ins_name, rgb)
            total_edits += n

        pkg, n = _patch_material_expression_vector_params(pkg, target_names, rgb)
        total_edits += n

    # 3. Wheels
    elif norm_slot == "Wheels":
        wheel_param = paint_param or "CustomColor"
        target_names = {
            wheel_param.lower(), "customcolor", "rimcolor", "paintcolor",
            "glowcolor", "emissivecolor", "wheelcolor",
        }

        pkg, promoted = _promote_wheel_painted_mics(pkg, mat_group_indices)
        if promoted:
            log.append(f"Promoted {promoted} painted static-switch Wheel MIC(s)")
            total_edits += promoted

        for idx, exp in enumerate(list(pkg.exports)):
            if pkg.export_class_name(exp) != "MaterialInstanceConstant":
                continue
            idx1 = idx + 1
            ename = pkg.resolve_name(exp.object_name).lower()
            if ename.startswith("tire_") and idx1 not in mat_group_indices:
                continue
            ins_name = wheel_param if (idx1 in mat_group_indices or "wheel" in ename or "rim" in ename) else None
            pkg, n = _upsert_mic_vector_param(pkg, idx, target_names, ins_name, rgb)
            total_edits += n

        pkg, n = _patch_material_expression_vector_params(pkg, target_names, rgb)
        total_edits += n
        pkg, n = _patch_particle_vector_params(pkg, target_names, rgb)
        total_edits += n

    # 4. Boosts, Goal Explosions, Trails, Toppers, Antennas (and any other slot)
    else:
        fx_param = paint_param or "CustomColor"
        target_names = {
            fx_param.lower(), "customcolor", "paintcolor", "innercolor", "outercolor",
            "dragoncolor", "dragoncolorsecond", "glowcolor", "beamcolor", "trailcolor",
            "flamecolor", "smokecolor", "primarycolor", "secondarycolor", "trimcolor",
        }

        pkg, switched = _switch_fx_to_painted_particle_systems(pkg)
        if switched:
            log.append(f"Switched {switched} ParticleSystemComponent(s) to *_Painted_PS")
            total_edits += switched

        pkg, n = _patch_particle_vector_params(pkg, target_names, rgb)
        if n:
            log.append(f"Patched {n} particle vector color distribution(s)")
            total_edits += n

        for idx, exp in enumerate(list(pkg.exports)):
            if pkg.export_class_name(exp) != "MaterialInstanceConstant":
                continue
            idx1 = idx + 1
            ins_name = fx_param if idx1 in mat_group_indices else None
            pkg, n = _upsert_mic_vector_param(pkg, idx, target_names, ins_name, rgb)
            total_edits += n

        pkg, n = _patch_material_expression_vector_params(pkg, target_names, rgb)
        total_edits += n

    # 5. Neutralize server-side PaintSettings override on sacrifice item
    pkg, cleared = _neutralize_paint_attributes(pkg, donor_pa_indices)
    if cleared:
        log.append(f"Neutralized {cleared} server PaintSettings attribute ref(s)")

    log.append(f"Total binary color edits applied: {total_edits}")
    return pkg, log
