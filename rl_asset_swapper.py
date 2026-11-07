#!/usr/bin/env python3
import argparse
import base64
import importlib
import importlib.util
import io
import json
import os
import shutil
import struct
import sys
import traceback
import hashlib
import hmac
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Dict, Iterable, List, Optional, Sequence, Tuple


# Dummy imports for PyInstaller to include dependencies of dynamically loaded rl_upk_editor
if False:
    import concurrent.futures
    import ctypes
    import hashlib
    import zlib
    import re
    import zipfile
    from cryptography.hazmat.backends import default_backend
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    from PIL import Image  # ensure Pillow is bundled

def get_pristine_path(path: Path) -> Path:
    """Return the immutable .bak baseline if present, otherwise the file itself."""
    bak = path.with_suffix(path.suffix + ".bak")
    return bak if bak.exists() else path


def _ensure_pristine_target(target_path: Path) -> bool:
    """If target_path was already swapped (.bak exists), remove the old swapped file and restore from .bak."""
    bak = target_path.with_suffix(target_path.suffix + ".bak")
    if bak.exists():
        if target_path.exists():
            target_path.unlink()
        shutil.copy2(bak, target_path)
        return True
    return False



@dataclass(frozen=True)
class Item:
    id: int
    product: str
    quality: str
    slot: str
    asset_package: str
    asset_path: str

    @property
    def package_stem(self) -> str:
        return Path(self.asset_package).stem

    @property
    def asset_parts(self) -> List[str]:
        return [p for p in self.asset_path.split(".") if p]

    @property
    def asset_base(self) -> str:
        parts = self.asset_parts
        return parts[0] if parts else self.package_stem.removesuffix("_SF")

    @property
    def thumbnail_package(self) -> str:
        return f"{self.asset_base}_T_SF.upk"

    @property
    def label(self) -> str:
        quality = f" / {self.quality}" if self.quality else ""
        slot = f" / {self.slot}" if self.slot else ""
        return f"[{self.id}] {self.product}{quality}{slot} ({self.asset_package})"


@dataclass
class SwapOptions:
    items_path: Path
    keys_path: Optional[Path]
    donor_dir: Path
    output_dir: Path
    key_source_dir: Optional[Path]
    include_thumbnails: bool
    preserve_header_offsets: bool
    overwrite: bool
    logger: Optional[Callable[[str], None]] = None
    color: Optional[str] = None


def script_dir() -> Path:
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parent


def default_path(names: Sequence[str]) -> Path:
    here = script_dir()
    for name in names:
        candidates = [
            here / name,
            here.parent / "python" / name,
            here.parent / "resources" / "python" / name,
            here.parent / "resources" / name,
            here.parent.parent / "python" / name,
            here.parent.parent / "resources" / "python" / name,
            Path.cwd() / name,
            Path.cwd() / "python" / name,
            Path.cwd() / "resources" / "python" / name,
        ]
        if getattr(sys, "_MEIPASS", None):
            candidates.insert(0, Path(sys._MEIPASS) / name)
            
        for candidate in candidates:
            if candidate.exists():
                return candidate
    return here / names[0]


def import_rl_upk_editor():
    try:
        return importlib.import_module("rl_upk_editor")
    except Exception:
        pass

    here = script_dir()
    names = ["rl_upk_editor.py", "rl_upk_editor(1).py"]
    candidates = []
    
    for name in names:
        candidates.extend([
            here / name,
            here.parent / "python" / name,
            here.parent / "resources" / "python" / name,
            here.parent / "resources" / name,
            here.parent.parent / "python" / name,
            here.parent.parent / "resources" / "python" / name,
            Path.cwd() / name,
            Path.cwd() / "python" / name,
            Path.cwd() / "resources" / "python" / name,
        ])
        if getattr(sys, "_MEIPASS", None):
            candidates.insert(0, Path(sys._MEIPASS) / name)

    last_err = None
    for candidate in candidates:
        if not candidate.exists():
            continue
        try:
            spec = importlib.util.spec_from_file_location("rl_upk_editor", candidate)
            if spec is None or spec.loader is None:
                continue
            module = importlib.util.module_from_spec(spec)
            sys.modules["rl_upk_editor"] = module
            spec.loader.exec_module(module)
            return module
        except Exception as e:
            last_err = e
            continue

    if last_err:
        raise ImportError(f"Failed to load rl_upk_editor from {len(candidates)} candidates. Last error: {last_err}")
    raise ImportError("Could not find rl_upk_editor.py in any search path.")


def load_items(path: Path) -> List[Item]:
    raw = json.loads(path.read_text(encoding="utf-8-sig"))
    # Support both CrunchyRL format {"Items":[...]} and new format {"items":[...]}
    rows = raw.get("Items") or raw.get("items") or (raw if isinstance(raw, list) else [])
    out: List[Item] = []
    for row in rows:
        try:
            # CrunchyRL keys: AssetPackage, AssetPath, ID, Product, Quality, Slot
            # New format keys: asset_package, asset_path, id, label/long_label, quality_label, slot
            pkg = str(row.get("AssetPackage") or row.get("asset_package") or "")
            asset_path = str(row.get("AssetPath") or row.get("asset_path") or "")
            if not pkg or not asset_path:
                continue
            out.append(Item(
                id=int(row.get("ID") or row.get("id") or 0),
                product=str(row.get("Product") or row.get("label") or row.get("long_label") or ""),
                quality=str(row.get("Quality") or row.get("quality_label") or ""),
                slot=str(row.get("Slot") or row.get("slot") or ""),
                asset_package=pkg,
                asset_path=asset_path,
            ))
        except Exception:
            continue
    out.sort(key=lambda x: (x.slot.lower(), x.product.lower(), x.id))
    return out


def find_item(items: Sequence[Item], value: str, slot: str = "") -> Item:
    value = str(value).strip()
    rows = [x for x in items if not slot or x.slot.lower() == slot.lower()]
    if value.isdigit():
        wanted = int(value)
        matches = [x for x in rows if x.id == wanted]
    else:
        q = value.lower()
        matches = [x for x in rows if q in x.product.lower() or q in x.asset_package.lower() or q in x.asset_path.lower()]
    if not matches:
        raise ValueError(f"No item matched {value!r}" + (f" in slot {slot!r}" if slot else ""))
    if len(matches) > 1:
        # 1. Try exact matches on product name or package (handle .upk suffix)
        exact = []
        for x in matches:
            p_low = x.product.lower()
            pkg_low = x.asset_package.lower()
            val_low = value.lower()
            if p_low == val_low or pkg_low == val_low or pkg_low.removesuffix(".upk") == val_low:
                exact.append(x)
        
        if len(exact) == 1:
            return exact[0]
            
        # 2. If still ambiguous, check if they are all functionally identical (same package & path)
        unique_assets = {(x.asset_package.lower(), x.asset_path.lower()) for x in (exact if exact else matches)}
        if len(unique_assets) == 1:
            return (exact if exact else matches)[0]

        raise ValueError("Ambiguous item match:\n" + "\n".join(x.label for x in matches[:20]))
    return matches[0]


def add_pair(pairs: List[Tuple[str, str]], old: str, new: str) -> None:
    old = (old or "").strip()
    new = (new or "").strip()
    if not old or not new or old == new:
        return
    if (old, new) not in pairs:
        pairs.append((old, new))


def infer_name_pairs(target: Item, donor: Item) -> List[Tuple[str, str]]:
    pairs: List[Tuple[str, str]] = []
    donor_parts = donor.asset_parts
    target_parts = target.asset_parts
    if len(donor_parts) == len(target_parts):
        for old, new in zip(donor_parts, target_parts):
            add_pair(pairs, old, new)
    else:
        if donor_parts and target_parts:
            add_pair(pairs, donor_parts[0], target_parts[0])
            add_pair(pairs, donor_parts[-1], target_parts[-1])
        for old, new in zip(donor_parts, target_parts):
            add_pair(pairs, old, new)
    add_pair(pairs, donor.package_stem, target.package_stem)
    return pairs


def infer_thumbnail_pairs(target: Item, donor: Item) -> List[Tuple[str, str]]:
    return [
        (f"{donor.asset_base}_T", f"{target.asset_base}_T"),
        (f"{donor.asset_base}_T_SF", f"{target.asset_base}_T_SF"),
    ]


def clean_name(text: str) -> str:
    return str(text).split("\x00", 1)[0].strip()


def find_name_indices(package, name: str) -> Tuple[List[int], bool]:
    exact = [n.index for n in package.names if clean_name(n.name) == name]
    if exact:
        return exact, False
    q = name.lower()
    fuzzy = [n.index for n in package.names if clean_name(n.name).lower() == q]
    return fuzzy, bool(fuzzy)


def name_exists(package, name: str) -> bool:
    return bool(find_name_indices(package, name)[0])


def parse_name_entry_spans(upk, package) -> List[Tuple[int, int, int, int]]:
    data = package.file_bytes
    pos = package.summary.name_offset
    spans: List[Tuple[int, int, int, int]] = []
    for _ in range(package.summary.name_count):
        start = pos
        if pos + 4 > len(data):
            raise ValueError("Name table is truncated")
        length = struct.unpack_from("<i", data, pos)[0]
        pos += 4
        if length > 0:
            byte_count = length
            pos += byte_count
        elif length < 0:
            byte_count = -length * 2
            pos += byte_count
        else:
            byte_count = 0
        flags_offset = pos
        pos += 8
        spans.append((start, flags_offset + 8, length, flags_offset))
    return spans


def make_fixed_fstring(old_len: int, new_text: str) -> Optional[bytes]:
    if old_len > 0:
        try:
            raw = new_text.encode("ascii")
        except UnicodeEncodeError:
            return None
        if len(raw) + 1 > old_len:
            return None
        return struct.pack("<i", old_len) + raw + b"\x00" + (b"\x00" * (old_len - len(raw) - 1))
    if old_len < 0:
        char_count = -old_len
        raw = new_text.encode("utf-16-le")
        if len(new_text) + 1 > char_count:
            return None
        pad_chars = char_count - len(new_text) - 1
        return struct.pack("<i", old_len) + raw + b"\x00\x00" + (b"\x00\x00" * pad_chars)
    return None


def fixed_rename_name_entry(upk, package, name_index: int, new_text: str):
    spans = parse_name_entry_spans(upk, package)
    start, end, old_len, flags_offset = spans[name_index]
    payload = make_fixed_fstring(old_len, new_text)
    if payload is None:
        return None, 0
    flags = package.file_bytes[flags_offset:flags_offset + 8]
    replacement = payload + flags
    if len(replacement) != end - start:
        raise ValueError("Fixed name replacement length mismatch")
    data = bytearray(package.file_bytes)
    data[start:end] = replacement
    result = upk.parse_decrypted_package_bytes(package.file_path, bytes(data))
    old_display = clean_name(package.names[name_index].name)
    pad = max(0, abs(old_len) - len(new_text) - 1)
    setattr(result, "_fixed_rename_index", name_index)
    setattr(result, "_fixed_rename_old", old_display)
    setattr(result, "_fixed_rename_new", new_text)
    setattr(result, "_fixed_rename_pad", pad)
    return result, pad


def patch_header_object_name_refs(upk, package, old_name: str, new_name: str) -> Tuple[object, List[str]]:
    old_indices, _ = find_name_indices(package, old_name)
    new_indices, _ = find_name_indices(package, new_name)
    if not old_indices or not new_indices:
        return package, []
    old_set = set(old_indices)
    new_idx = new_indices[0]
    data = bytearray(package.file_bytes)
    log: List[str] = []

    if hasattr(upk, "get_export_entry_offsets"):
        offsets = upk.get_export_entry_offsets(package)
        for exp, off in zip(package.exports, offsets):
            if exp.object_name.name_index in old_set:
                data[off + 12:off + 16] = struct.pack("<i", new_idx)
                log.append(f"PATCHED: export[{exp.table_index}] object_name {old_name!r} -> existing {new_name!r}")

    import_off = package.summary.import_offset
    for imp in package.imports:
        off = import_off + imp.table_index * 28
        if imp.object_name.name_index in old_set:
            data[off + 20:off + 24] = struct.pack("<i", new_idx)
            log.append(f"PATCHED: import[{imp.table_index}] object_name {old_name!r} -> existing {new_name!r}")

    if not log:
        return package, []
    return upk.parse_decrypted_package_bytes(package.file_path, bytes(data)), log


def apply_name_pairs(upk, package, pairs: Sequence[Tuple[str, str]], preserve_header_offsets: bool) -> Tuple[object, List[str]]:
    current = package
    log: List[str] = []
    for old, new in pairs:
        indices, case_match = find_name_indices(current, old)
        if not indices:
            log.append(f"MISS: no name-table entry matching {old!r}")
            continue
        if case_match:
            log.append(f"CASE: matched {old!r} case-insensitively")

        # If the target name already exists elsewhere in the donor's name table,
        # freeing it (FREEDNAME) breaks every import/export that legitimately
        # references it (e.g. 'Boost_Standard' in a Gold Rush package). This
        # causes "Bad name index" crashes in the game. Block the swap instead.
        colliding_indices, _ = find_name_indices(current, new)
        if colliding_indices:
            raise ValueError(
                f"Cannot swap: the visual item's package already references '{new}' internally. "
                f"Try a different visual item."
            )

        # Now force the physical text replacement so body and header stay perfectly synced
        for idx in indices:
            old_actual = clean_name(current.names[idx].name)
            if preserve_header_offsets:
                fixed, pad = fixed_rename_name_entry(upk, current, idx, new)
                if fixed is not None:
                    current = fixed
                    log.append(f"FIXED: name[{idx}] {old_actual!r} -> {new!r} in-place; preserved header offsets; pad={pad}.")
                    continue
            try:
                current = upk.rename_name_entry(current, idx, new)
                log.append(f"RENAMED: name[{idx}] {old_actual!r} -> {new!r}; header offsets may change.")
            except Exception as e:
                log.append(f"ERROR: could not rename {old_actual!r}: {e}")
                
    return current, log


def load_provider(upk, keys_path: Optional[Path], donor_path: Path, script_path: Path):
    if keys_path and keys_path.exists():
        return upk.DecryptionProvider(str(keys_path)), keys_path
    found = upk.find_keys_path(script_path, donor_path) if hasattr(upk, "find_keys_path") else None
    if found:
        return upk.DecryptionProvider(str(found)), Path(found)
    return None, None


def resolve_with_optional_keys(upk, input_path: Path, temp_dir: Path, keys_path: Optional[Path]):
    if not keys_path:
        return upk.resolve_input_package(input_path, temp_dir, script_dir())
    old_find = getattr(upk, "find_keys_path", None)
    if old_find is None:
        return upk.resolve_input_package(input_path, temp_dir, script_dir())
    def forced(_script_dir, _selected_file):
        return keys_path
    upk.find_keys_path = forced
    try:
        return upk.resolve_input_package(input_path, temp_dir, script_dir())
    finally:
        upk.find_keys_path = old_find


def summary_line(package) -> str:
    return f"names={package.summary.name_count}, depends={package.summary.depends_offset}, first_export={package.exports[0].serial_offset if package.exports else 0}"



def build_reencrypted_package_with_output_key(upk, original_encrypted_path: Path, modified_decrypted_bytes: bytes, provider, output_path: Path, output_key: bytes) -> Path:
    summary, meta, original_encrypted_data, donor_key = upk.find_valid_key(original_encrypted_path, provider)
    modified_summary = upk.parse_file_summary(io.BytesIO(modified_decrypted_bytes))
    original_plain = bytearray(upk.DecryptionProvider.decrypt_ecb(donor_key, original_encrypted_data))
    original_chunks = upk.parse_rl_compressed_chunks(bytes(original_plain), meta.compressed_chunks_offset, summary.licensee_version)
    if not original_chunks:
        raise ValueError("No compressed chunks were found in original encrypted header")

    new_chunk_table_offset = modified_summary.depends_offset - modified_summary.name_offset
    patch_limit = max(0, new_chunk_table_offset)
    chunk_shift = modified_summary.depends_offset - original_chunks[0].uncompressed_offset

    rebuilt_chunks = []
    rebuilt_chunk_payloads = []
    chunk_table_placeholder = upk.serialize_rl_chunk_table([
        upk.FCompressedChunk(0, 0, 0, 0) for _ in original_chunks
    ], summary.licensee_version)
    required_plain_len = new_chunk_table_offset + len(chunk_table_placeholder)
    encrypted_plain_len = (required_plain_len + 15) & ~15
    header_plain = bytearray(encrypted_plain_len)
    copy_len = min(len(original_plain), encrypted_plain_len)
    header_plain[:copy_len] = original_plain[:copy_len]

    original_gap_start_calc = summary.name_offset + len(original_encrypted_data)
    original_gap_end_calc = original_chunks[0].compressed_offset
    actual_garbage_size = original_gap_end_calc - original_gap_start_calc

    new_total_header_size = modified_summary.name_offset + encrypted_plain_len + actual_garbage_size
    current_compressed_offset = new_total_header_size
    for i, chunk in enumerate(original_chunks):
        start = chunk.uncompressed_offset + chunk_shift
        if i + 1 < len(original_chunks):
            end = original_chunks[i + 1].uncompressed_offset + chunk_shift
            if end > len(modified_decrypted_bytes):
                raise ValueError("Modified decrypted package changed size too early for the rebuilt chunk layout")
        else:
            end = len(modified_decrypted_bytes)
        if end < start:
            raise ValueError("Invalid rebuilt chunk bounds")
        payload = upk.compress_chunk_payload(modified_decrypted_bytes[start:end])
        rebuilt_chunk_payloads.append(payload)
        rebuilt_chunks.append(upk.FCompressedChunk(
            uncompressed_offset=start,
            uncompressed_size=end - start,
            compressed_offset=current_compressed_offset,
            compressed_size=len(payload),
        ))
        current_compressed_offset += len(payload)

    if patch_limit > len(header_plain):
        raise ValueError("Modified decrypted header exceeds encrypted header capacity")
    if patch_limit > 0:
        header_plain[:patch_limit] = modified_decrypted_bytes[summary.name_offset:modified_summary.depends_offset]

    chunk_table = upk.serialize_rl_chunk_table(rebuilt_chunks, summary.licensee_version)
    table_end = new_chunk_table_offset + len(chunk_table)
    if table_end > len(header_plain):
        raise ValueError("Rebuilt compressed chunk table does not fit inside encrypted header")
    header_plain[new_chunk_table_offset:table_end] = chunk_table
    encrypted_header = upk.DecryptionProvider.encrypt_ecb(output_key, bytes(header_plain))

    original_bytes = Path(original_encrypted_path).read_bytes()
    prefix = bytearray(original_bytes[:summary.name_offset])
    summary_offsets = upk._find_summary_offsets(modified_decrypted_bytes)
    upk.patch_i32_le(prefix, summary_offsets["total_header_size_offset"], new_total_header_size)
    upk.patch_i32_le(prefix, summary_offsets["name_count_offset"], modified_summary.name_count)
    upk.patch_i32_le(prefix, summary_offsets["name_offset_offset"], modified_summary.name_offset)
    upk.patch_i32_le(prefix, summary_offsets["export_count_offset"], modified_summary.export_count)
    upk.patch_i32_le(prefix, summary_offsets["export_offset_offset"], modified_summary.export_offset)
    upk.patch_i32_le(prefix, summary_offsets["import_count_offset"], modified_summary.import_count)
    upk.patch_i32_le(prefix, summary_offsets["import_offset_offset"], modified_summary.import_offset)
    upk.patch_i32_le(prefix, summary_offsets["depends_offset_offset"], modified_summary.depends_offset)
    upk.patch_i32_le(prefix, summary_offsets["import_export_guids_offset_offset"], modified_summary.import_export_guids_offset)
    if "thumbnail_table_offset_offset" in summary_offsets:
        upk.patch_i32_le(prefix, summary_offsets["thumbnail_table_offset_offset"], modified_summary.thumbnail_table_offset)
    upk._patch_generation_counts(prefix, summary_offsets, modified_summary.export_count, modified_summary.name_count)
    with original_encrypted_path.open("rb") as src:
        meta_offsets = upk._find_file_compression_metadata_offsets(src)
    upk.patch_i32_le(prefix, meta_offsets["compressed_chunks_offset_offset"], new_chunk_table_offset)
    if rebuilt_chunks:
        upk.patch_i32_le(prefix, meta_offsets["last_block_size_offset"], rebuilt_chunks[-1].uncompressed_size)

    output = bytearray()
    output += prefix
    output += encrypted_header
    gap_start = modified_summary.name_offset + len(encrypted_header)
    original_gap_start = summary.name_offset + len(original_encrypted_data)
    original_gap_end = original_chunks[0].compressed_offset
    gap_bytes = original_bytes[original_gap_start:original_gap_end]
    output += gap_bytes
    for payload in rebuilt_chunk_payloads:
        output += payload

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(output)
    return output_path

_keys_map: Optional[dict] = None

def _load_keys_map() -> dict:
    global _keys_map
    if _keys_map is not None:
        return _keys_map
    try:
        map_path = default_path(("keys_map.json",))
        if map_path.exists():
            import json
            _keys_map = json.loads(map_path.read_text(encoding="utf-8"))
        else:
            _keys_map = {}
    except Exception:
        _keys_map = {}
    return _keys_map


def _get_exact_key(target_key_path: Path) -> Optional[bytes]:
    """Look up the exact AES key for a UPK file using the Shift key map."""
    keys_map = _load_keys_map()
    if not keys_map:
        return None
    # Derive package name: Boost_Bubble_SF.upk(.bak) -> boost_bubble_sf, boost_bubble
    stem = target_key_path.name.lower()
    if stem.endswith('.bak'):
        stem = stem[:-4]
    if stem.endswith('.upk'):
        stem = stem[:-4]
    key_b64 = keys_map.get(stem) or keys_map.get(stem.removesuffix('_sf'))
    if not key_b64:
        return None
    try:
        return base64.b64decode(key_b64)
    except Exception:
        return None


# ── Low-level UPK helpers (mirrors the Rust engine exactly) ──────────────────

PACKAGE_FILE_TAG = 0x9E2A83C1

@dataclass
class _UPKPrefix:
    total_header_size: int
    package_flags: int
    name_count: int
    name_offset: int
    export_count: int
    export_offset: int
    import_offset: int
    depends_offset: int
    garbage_size: int
    compressed_chunks_offset: int
    meta_file_offset: int  # byte position of garbage_size field in raw file
    ctr_nonce: Optional[bytes] = None
    licensee_version: int = 34


def _read_fstring_at(data: bytes, pos: int) -> Tuple[str, int]:
    flen = struct.unpack_from('<i', data, pos)[0]; pos += 4
    if flen > 0:
        raw = data[pos:pos + flen]; pos += flen
        return raw.rstrip(b'\x00').decode('utf-8', errors='replace'), pos
    if flen < 0:
        bc = (-flen) * 2; raw = data[pos:pos + bc]; pos += bc
        return raw.decode('utf-16-le', errors='replace').rstrip('\x00'), pos
    return '', pos


def _parse_upk_prefix(data: bytes) -> _UPKPrefix:
    if struct.unpack_from('<I', data, 0)[0] != PACKAGE_FILE_TAG:
        raise ValueError("Not a valid UPK file")
    _, licensee_version = struct.unpack_from('<HH', data, 4)
    pos = 8  # skip tag + versions
    total_header_size = struct.unpack_from('<i', data, pos)[0]; pos += 4
    _, pos = _read_fstring_at(data, pos)          # folder_name
    package_flags = struct.unpack_from('<I', data, pos)[0]; pos += 4
    name_count  = struct.unpack_from('<i', data, pos)[0]; pos += 4
    name_offset = struct.unpack_from('<i', data, pos)[0]; pos += 4
    export_count = struct.unpack_from('<i', data, pos)[0]; pos += 4
    export_offset = struct.unpack_from('<i', data, pos)[0]; pos += 4
    import_count = struct.unpack_from('<i', data, pos)[0]; pos += 4  # noqa: F841
    import_offset = struct.unpack_from('<i', data, pos)[0]; pos += 4
    depends_offset = struct.unpack_from('<i', data, pos)[0]; pos += 4
    pos += 16                                      # import_export_guids + counts + thumbnail
    pos += 16                                      # GUID
    gen_count = struct.unpack_from('<i', data, pos)[0]; pos += 4
    pos += gen_count * 12                          # generations
    pos += 12                                      # engine_version, cooker_version, compression_flags
    std_chunks = struct.unpack_from('<i', data, pos)[0]; pos += 4
    pos += std_chunks * 16
    pos += 4                                       # PackageSource
    add_count = struct.unpack_from('<i', data, pos)[0]; pos += 4
    for _ in range(add_count):
        _, pos = _read_fstring_at(data, pos)
    tex_count = struct.unpack_from('<i', data, pos)[0]; pos += 4
    for _ in range(tex_count):
        pos += 20
        inner = struct.unpack_from('<i', data, pos)[0]; pos += 4
        pos += inner * 4
    meta_file_offset = pos
    garbage_size = struct.unpack_from('<i', data, pos)[0]; pos += 4
    compressed_chunks_offset = struct.unpack_from('<i', data, pos)[0]; pos += 4
    pos += 4                                       # last_block_size
    ctr_nonce = data[pos:pos + 12] if (package_flags & 0x800) != 0 else None
    return _UPKPrefix(
        total_header_size=total_header_size, package_flags=package_flags,
        name_count=name_count, name_offset=name_offset,
        export_count=export_count, export_offset=export_offset,
        import_offset=import_offset, depends_offset=depends_offset,
        garbage_size=garbage_size, compressed_chunks_offset=compressed_chunks_offset,
        meta_file_offset=meta_file_offset, ctr_nonce=ctr_nonce,
        licensee_version=licensee_version,
    )


def _find_summary_offsets(data: bytes) -> dict:
    pos = 8
    total_off = pos; pos += 4
    flen = struct.unpack_from('<i', data, pos)[0]; pos += 4
    if flen > 0: pos += flen
    elif flen < 0: pos += (-flen) * 2
    pos += 4
    name_count_off = pos; pos += 4
    name_off_off   = pos; pos += 4
    export_count_off = pos; pos += 4
    export_off_off = pos; pos += 4
    pos += 4
    import_off_off = pos; pos += 4
    depends_off_off = pos; pos += 4
    guids_off_off = pos; pos += 4
    pos += 8
    thumb_off_off = pos; pos += 4
    pos += 16
    gen_count = struct.unpack_from('<i', data, pos)[0]; pos += 4
    gen_entries_off = pos
    return {
        'total_header_size': total_off,
        'name_count': name_count_off,
        'name_offset': name_off_off,
        'export_count': export_count_off,
        'export_offset': export_off_off,
        'import_offset': import_off_off,
        'depends_offset': depends_off_off,
        'import_export_guids_offset': guids_off_off,
        'thumbnail_table_offset': thumb_off_off,
        'generation_count': gen_count,
        'generation_entries': gen_entries_off,
    }


def _patch_i32(data: bytearray, offset: int, value: int) -> None:
    if offset + 4 <= len(data):
        struct.pack_into('<i', data, offset, value)


def _aes_ecb_decrypt(key: bytes, data: bytes) -> bytes:
    from cryptography.hazmat.backends import default_backend
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    c = Cipher(algorithms.AES(key), modes.ECB(), backend=default_backend())
    d = c.decryptor()
    return d.update(data) + d.finalize()


def _aes_ecb_encrypt(key: bytes, data: bytes) -> bytes:
    from cryptography.hazmat.backends import default_backend
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    pad = (16 - len(data) % 16) % 16
    data = data + b'\x00' * pad
    c = Cipher(algorithms.AES(key), modes.ECB(), backend=default_backend())
    e = c.encryptor()
    return e.update(data) + e.finalize()


def _aes_ctr_crypt(key: bytes, nonce12: bytes, data: bytes) -> bytes:
    from cryptography.hazmat.backends import default_backend
    from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
    iv = nonce12 + b'\x00\x00\x00\x00'
    c = Cipher(algorithms.AES(key), modes.CTR(iv), backend=default_backend())
    ctx = c.encryptor()
    return ctx.update(data) + ctx.finalize()


def _walk_name_table(header: bytes, name_count: int) -> List[dict]:
    slots, pos = [], 0
    for _ in range(name_count):
        if pos + 4 > len(header):
            raise ValueError("Name table truncated")
        flen = struct.unpack_from('<i', header, pos)[0]
        cap  = flen if flen > 0 else (-flen * 2) if flen < 0 else 0
        if pos + 4 + cap + 8 > len(header):
            raise ValueError("Name entry overrun")
        if flen > 0:
            name = header[pos+4:pos+4+cap].rstrip(b'\x00').decode('utf-8', errors='replace')
        elif flen < 0:
            name = header[pos+4:pos+4+cap].decode('utf-16-le', errors='replace').rstrip('\x00')
        else:
            name = ''
        flags_off = pos + 4 + cap
        flags = struct.unpack_from('<Q', header, flags_off)[0]
        entry_end = flags_off + 8
        slots.append({'pos': pos, 'flen': flen, 'cap': cap,
                      'name': name, 'flags': flags, 'end': entry_end})
        pos = entry_end
    return slots


def _serialize_name_entry(name: str, flags: int) -> bytes:
    raw = name.encode('utf-8')
    return struct.pack('<i', len(raw) + 1) + raw + b'\x00' + struct.pack('<Q', flags)


class _NameTooLong(Exception):
    pass


def _apply_inplace(header: bytes, name_count: int, pairs: List[Tuple[str, str]]) -> bytes:
    data  = bytearray(header)
    slots = _walk_name_table(bytes(data), name_count)
    for old_str, new_str in pairs:
        idxs = [i for i, s in enumerate(slots) if s['name'].lower() == old_str.lower()]
        if not idxs:
            continue
        if any(i not in idxs and s['name'].lower() == new_str.lower() for i, s in enumerate(slots)):
            raise ValueError(
                f"Cannot swap: package already references '{new_str}' internally. "
                f"Try a different visual item.")
        for idx in idxs:
            s = slots[idx]
            if s['flen'] < 0:
                raise ValueError(f"Name '{old_str}' uses UTF-16; rename not supported.")
            needed = len(new_str.encode('utf-8')) + 1
            if needed > s['cap']:
                raise _NameTooLong()
            start = s['pos'] + 4
            nb = new_str.encode('utf-8')
            for i in range(s['cap']):
                data[start + i] = nb[i] if i < len(nb) else 0
            slots[idx]['name'] = new_str
    return bytes(data)


def _apply_header_renames(
    header: bytes,
    import_off: int, export_off: int, depends_off: int,
    name_count: int,
    pairs: List[Tuple[str, str]],
    export_count: int = 0,
    compressed_chunks_offset: int = 0,
    chunk_stride: int = 36,
) -> Tuple[bytes, int]:
    """
    Rename name table entries in decrypted header (name table at byte 0).
    Tries in-place first; rebuilds tables if name is longer.
    Returns (new_header, delta_bytes).
    """
    try:
        return _apply_inplace(header, name_count, pairs), 0
    except _NameTooLong:
        pass

    cur = bytearray(header)
    ci, ce, cd = import_off, export_off, depends_off

    for old_str, new_str in pairs:
        slots = _walk_name_table(bytes(cur), name_count)
        idxs = [i for i, s in enumerate(slots) if s['name'].lower() == old_str.lower()]
        if not idxs:
            continue
        if any(i not in idxs and s['name'].lower() == new_str.lower() for i, s in enumerate(slots)):
            raise ValueError(
                f"Cannot swap: package already references '{new_str}' internally. "
                f"Try a different visual item.")
        for idx in idxs:
            s = slots[idx]
            if s['flen'] < 0:
                raise ValueError(f"Name '{old_str}' uses UTF-16; rename not supported.")
            old_nt   = bytes(cur[:ci])
            imp_tbl  = bytes(cur[ci:ce])
            exp_tbl  = bytes(cur[ce:cd])
            beyond   = bytes(cur[cd:])
            new_nt   = bytearray()
            for i2, s2 in enumerate(slots):
                new_nt += _serialize_name_entry(new_str, s2['flags']) if i2 == idx \
                          else old_nt[s2['pos']:s2['end']]
            delta = len(new_nt) - len(old_nt)
            cur = bytearray(new_nt) + bytearray(imp_tbl) + bytearray(exp_tbl) + bytearray(beyond)
            ci += delta; ce += delta; cd += delta

    header_delta = len(cur) - len(header)
    if header_delta != 0:
        epos = ce
        for _ in range(export_count):
            if epos + 52 > cd:
                break
            serial_off = struct.unpack_from('<q', cur, epos + 36)[0]
            if serial_off > 0:
                struct.pack_into('<q', cur, epos + 36, serial_off + header_delta)
            net_cnt = struct.unpack_from('<i', cur, epos + 48)[0]
            epos += 52 + net_cnt * 4 + 20

        c_off = compressed_chunks_offset + header_delta
        if 0 <= c_off and c_off + 4 <= len(cur):
            num_chunks = struct.unpack_from('<i', cur, c_off)[0]
            for i in range(num_chunks):
                entry_off = c_off + 4 + i * chunk_stride
                if entry_off + chunk_stride > len(cur):
                    break
                u_off = struct.unpack_from('<q', cur, entry_off)[0]
                struct.pack_into('<q', cur, entry_off, u_off + header_delta)

    return bytes(cur), header_delta


def _ue3_split_fname(text: str) -> Tuple[str, int]:
    """Split a UE3 FName string into (base_name, stored_instance) on-disk format."""
    if '_' in text:
        base, suffix = text.rsplit('_', 1)
        if base and suffix.isdigit() and (suffix == '0' or not suffix.startswith('0')):
            return base, int(suffix) + 1
    return text, 0


def _extract_target_fnames_from_upk(
    target_upk_path: Path,
    target_key: Optional[bytes],
    target_asset_path: str,
    target_pkg_stem: str,
) -> Tuple[List[Tuple[str, int]], Tuple[str, int]]:
    """
    Read the target UPK's own decrypted ExportTable when available so every segment's
    exact (base_name, stored_instance) matches Psyonix's cooked header bit-for-bit.
    Falls back to UE3 FName::SplitNameWithCheck when target UPK is absent.
    """
    fallback_chain = [_ue3_split_fname(seg) for seg in target_asset_path.split('.') if seg]
    fallback_sf = _ue3_split_fname(target_pkg_stem)
    pristine_target = get_pristine_path(target_upk_path)
    if not pristine_target.exists() or target_key is None:
        return fallback_chain, fallback_sf

    try:
        with open(pristine_target, 'rb') as fh:
            raw = fh.read(65536)
        pfx = _parse_upk_prefix(raw)
        enc_sz = pfx.total_header_size - pfx.garbage_size - pfx.name_offset
        if enc_sz <= 0:
            return fallback_chain, fallback_sf
        need = pfx.name_offset + ((enc_sz + 15) & ~15)
        if need > len(raw):
            with open(pristine_target, 'rb') as fh:
                raw = fh.read(need)
        eb = raw[pfx.name_offset:need]
        plain = (
            _aes_ctr_crypt(target_key, pfx.ctr_nonce, eb)
            if pfx.ctr_nonce is not None
            else _aes_ecb_decrypt(target_key, eb)
        )[:enc_sz]
        c_off = pfx.compressed_chunks_offset
        if c_off < 0 or c_off + 8 > len(plain):
            return fallback_chain, fallback_sf
        num_c, u0 = struct.unpack_from('<ii', plain, c_off)
        if num_c < 1 or u0 != pfx.depends_offset:
            return fallback_chain, fallback_sf

        slots = _walk_name_table(plain, pfx.name_count)
        ci = pfx.import_offset - pfx.name_offset
        ce = pfx.export_offset - pfx.name_offset
        imp_cnt = (ce - ci) // 28
        imp_names = [slots[struct.unpack_from('<iiiiiii', plain, ci + i * 28)[5]]['name'] for i in range(imp_cnt)]

        exps = []
        epos = ce
        for _ in range(pfx.export_count):
            cls_i, _sup, out_i, nm_i, inst_i, _arch = struct.unpack_from('<iiiiii', plain, epos)
            nc = struct.unpack_from('<i', plain, epos + 48)[0]
            cname = imp_names[-cls_i - 1] if (cls_i < 0 and -cls_i - 1 < len(imp_names)) else 'Class'
            base_nm = slots[nm_i]['name']
            fmt_nm = f"{base_nm}_{inst_i - 1}" if inst_i > 0 else base_nm
            exps.append((cname, out_i, base_nm, inst_i, fmt_nm))
            epos += 52 + nc * 4 + 20

        def full_path_and_chain(idx1: int) -> Tuple[str, List[Tuple[str, int]]]:
            pts_str, pts_fn, seen = [], [], set()
            cur = idx1
            while cur > 0 and cur not in seen and cur <= len(exps):
                seen.add(cur)
                _, oi, bnm, ini, fnm = exps[cur - 1]
                pts_str.append(fnm)
                pts_fn.append((bnm, ini))
                cur = oi
            pts_str.reverse()
            pts_fn.reverse()
            return '.'.join(pts_str), pts_fn

        matched_chain = None
        target_low = target_asset_path.lower()
        for i in range(len(exps)):
            fp, ch = full_path_and_chain(i + 1)
            if fp.lower() == target_low and not fp.lower().startswith('productslots.'):
                matched_chain = ch
                break
        if matched_chain is None:
            for i in range(len(exps)):
                fp, ch = full_path_and_chain(i + 1)
                if fp.lower() == target_low:
                    matched_chain = ch
                    break
        if matched_chain is None:
            pa_cands = []
            for i, (cn, _, _, _, _) in enumerate(exps):
                if cn.lower().startswith('productasset_') and not cn.lower().startswith('productassetreference'):
                    fp, ch = full_path_and_chain(i + 1)
                    if not fp.lower().startswith('productslots.'):
                        pa_cands.append((fp, ch))
            if len(pa_cands) == 1:
                matched_chain = pa_cands[0][1]

        sf_fname = fallback_sf
        for cn, oi, bnm, ini, fnm in exps:
            if cn.lower() == 'package' and oi == 0 and fnm.lower() == target_pkg_stem.lower():
                sf_fname = (bnm, ini)
                break

        return (matched_chain or fallback_chain), sf_fname
    except Exception:
        return fallback_chain, fallback_sf


def _rewire_header_surgical(
    header: bytes,
    import_off: int,
    export_off: int,
    depends_off: int,
    name_count: int,
    export_count: int,
    compressed_chunks_offset: int,
    donor_asset_path: str,
    donor_pkg_stem: str,
    target_chain_fnames: Sequence[Tuple[str, int]],
    target_sf_fname: Tuple[str, int],
    chunk_stride: int = 36,
) -> Tuple[bytes, int, int, int, int, int, int]:
    """
    Universal surgical ExportTable rewiring:
      - Locates the exact donor ProductAsset_*_TA export matching donor_asset_path.
      - Reuses existing NameTable entries (preventing all internal name collisions) or appends new ones.
      - Reuses or appends Core.Package ExportTable entries to match any target AssetPath depth (2, 3, or 4 parts).
      - Evicts intra-package shadow exports when swapping between items inside the same shared UPK.
      - Leaves all textures, materials, meshes, and imports intact.
    Returns (new_header, header_delta, new_name_count, new_export_count, new_import_rel, new_export_rel, new_depends_rel).
    """
    if not target_chain_fnames:
        raise ValueError("Target AssetPath cannot be empty")

    name_slots = _walk_name_table(header, name_count)
    names_list = [s['name'] for s in name_slots]
    nt_end = name_slots[-1]['end'] if name_slots else 0
    nt_tail = header[nt_end:import_off]
    extra_nt = bytearray()

    imp_blob = header[import_off:export_off]
    imp_count = len(imp_blob) // 28
    imp_obj_names: List[str] = []
    pkg_import_cls_idx: Optional[int] = None
    for i in range(imp_count):
        _cp_n, _cp_i, cn_n, _cn_i, _out_i, on_n, on_i = struct.unpack_from('<iiiiiii', imp_blob, i * 28)
        on_base = names_list[on_n] if 0 <= on_n < len(names_list) else ''
        on_fmt = f"{on_base}_{on_i - 1}" if on_i > 0 else on_base
        cn_base = names_list[cn_n] if 0 <= cn_n < len(names_list) else ''
        imp_obj_names.append(on_fmt)
        if pkg_import_cls_idx is None and on_base.lower() == 'package' and cn_base.lower() == 'class':
            pkg_import_cls_idx = -(i + 1)

    exps: List[dict] = []
    epos = export_off
    for idx in range(export_count):
        if epos + 52 > depends_off:
            raise ValueError("ExportTable truncated before DependsOffset")
        cls_i, _sup_i, out_i, nm_i, inst_i, _arch_i = struct.unpack_from('<iiiiii', header, epos)
        obj_flags = struct.unpack_from('<Q', header, epos + 24)[0]
        serial_sz = struct.unpack_from('<i', header, epos + 32)[0]
        serial_off = struct.unpack_from('<q', header, epos + 36)[0]
        exp_flags = struct.unpack_from('<i', header, epos + 44)[0]
        net_cnt = struct.unpack_from('<i', header, epos + 48)[0]
        entry_len = 52 + net_cnt * 4 + 20
        pkg_flags = struct.unpack_from('<i', header, epos + 52 + net_cnt * 4 + 16)[0]
        exps.append({
            'idx1': idx + 1,
            'cls_i': cls_i,
            'out_i': out_i,
            'nm_i': nm_i,
            'inst_i': inst_i,
            'obj_flags': obj_flags,
            'serial_sz': serial_sz,
            'serial_off': serial_off,
            'exp_flags': exp_flags,
            'pkg_flags': pkg_flags,
            'raw': bytearray(header[epos:epos + entry_len]),
        })
        epos += entry_len
    exp_tail = header[epos:depends_off]

    def fmt_name(nm_i: int, inst_i: int) -> str:
        base = names_list[nm_i] if 0 <= nm_i < len(names_list) else ''
        return f"{base}_{inst_i - 1}" if inst_i > 0 else base

    def cls_name(cls_i: int) -> str:
        if cls_i < 0 and -cls_i - 1 < len(imp_obj_names):
            return imp_obj_names[-cls_i - 1]
        if cls_i == 0:
            return 'Class'
        if 0 < cls_i <= len(exps):
            return fmt_name(exps[cls_i - 1]['nm_i'], exps[cls_i - 1]['inst_i'])
        return ''

    def full_path(idx1: int) -> str:
        pts, seen = [], set()
        cur = idx1
        while cur > 0 and cur not in seen and cur <= len(exps):
            seen.add(cur)
            e = exps[cur - 1]
            pts.append(fmt_name(e['nm_i'], e['inst_i']))
            cur = e['out_i']
        pts.reverse()
        return '.'.join(pts)

    donor_low = donor_asset_path.lower()
    donor_pa_exp = None
    for e in exps:
        fp = full_path(e['idx1'])
        if fp.lower() == donor_low and not fp.lower().startswith('productslots.'):
            donor_pa_exp = e
            break
    if donor_pa_exp is None:
        for e in exps:
            if full_path(e['idx1']).lower() == donor_low:
                donor_pa_exp = e
                break
    if donor_pa_exp is None:
        donor_leaf = donor_asset_path.split('.')[-1].lower()
        pa_candidates = [
            e for e in exps
            if cls_name(e['cls_i']).lower().startswith('productasset_')
            and not cls_name(e['cls_i']).lower().startswith('productassetreference')
            and not full_path(e['idx1']).lower().startswith('productslots.')
        ]
        for e in pa_candidates:
            if fmt_name(e['nm_i'], e['inst_i']).lower() == donor_leaf:
                donor_pa_exp = e
                break
        if donor_pa_exp is None and pa_candidates:
            donor_pa_exp = pa_candidates[0]
    if donor_pa_exp is None:
        raise ValueError(f"Could not locate donor ProductAsset export '{donor_asset_path}'")

    default_name_flags = (
        name_slots[donor_pa_exp['nm_i']]['flags']
        if 0 <= donor_pa_exp['nm_i'] < len(name_slots)
        else 0x0007001000000000
    )

    def ensure_fname(base_str: str, stored_inst: int) -> Tuple[int, int]:
        low = base_str.lower()
        for idx, existing in enumerate(names_list):
            if existing.lower() == low:
                return idx, stored_inst
        extra_nt.extend(_serialize_name_entry(base_str, default_name_flags))
        names_list.append(base_str)
        return len(names_list) - 1, stored_inst

    # Find template Core.Package export (12-byte serialized None payload)
    tpl_pkg = None
    if 0 < donor_pa_exp['out_i'] <= len(exps):
        cand = exps[donor_pa_exp['out_i'] - 1]
        if cls_name(cand['cls_i']).lower() == 'package' and cand['serial_sz'] == 12 and cand['serial_off'] > 0:
            tpl_pkg = cand
    if tpl_pkg is None:
        for e in exps:
            if cls_name(e['cls_i']).lower() == 'package' and e['serial_sz'] == 12 and e['serial_off'] > 0:
                tpl_pkg = e
                break

    # Build or reuse Core.Package outer_index chain for target_chain_fnames[:-1]
    cur_outer = 0
    for seg_base, seg_inst in target_chain_fnames[:-1]:
        existing_pkg = None
        for e in exps:
            if (
                cls_name(e['cls_i']).lower() == 'package'
                and e['out_i'] == cur_outer
                and names_list[e['nm_i']].lower() == seg_base.lower()
                and e['inst_i'] == seg_inst
            ):
                existing_pkg = e
                break
        if existing_pkg is not None:
            cur_outer = existing_pkg['idx1']
        else:
            seg_nm_i, seg_inst_i = ensure_fname(seg_base, seg_inst)
            p_cls = tpl_pkg['cls_i'] if tpl_pkg is not None else (pkg_import_cls_idx or -1)
            p_oflags = tpl_pkg['obj_flags'] if tpl_pkg is not None else 0x0007000400000000
            p_ssz = tpl_pkg['serial_sz'] if tpl_pkg is not None else 0
            p_soff = tpl_pkg['serial_off'] if tpl_pkg is not None else 0
            p_eflags = tpl_pkg['exp_flags'] if tpl_pkg is not None else 1
            p_pflags = 0x20000005 if cur_outer == 0 else 0
            raw_pkg = bytearray(struct.pack(
                '<iiiiiiQiqiiIIIIi',
                p_cls, 0, cur_outer, seg_nm_i, seg_inst_i, 0,
                p_oflags, p_ssz, p_soff, p_eflags, 0,
                0, 0, 0, 0, p_pflags,
            ))
            new_idx1 = len(exps) + 1
            exps.append({
                'idx1': new_idx1,
                'cls_i': p_cls,
                'out_i': cur_outer,
                'nm_i': seg_nm_i,
                'inst_i': seg_inst_i,
                'obj_flags': p_oflags,
                'serial_sz': p_ssz,
                'serial_off': p_soff,
                'exp_flags': p_eflags,
                'pkg_flags': p_pflags,
                'raw': raw_pkg,
            })
            cur_outer = new_idx1

    # Evict any sibling export that would shadow target's leaf path (intra-shared-UPK swaps)
    leaf_base, leaf_inst = target_chain_fnames[-1]
    leaf_nm_i, leaf_inst_i = ensure_fname(leaf_base, leaf_inst)
    for other in exps:
        if (
            other['idx1'] != donor_pa_exp['idx1']
            and other['out_i'] == cur_outer
            and names_list[other['nm_i']].lower() == leaf_base.lower()
            and other['inst_i'] == leaf_inst_i
        ):
            ev_nm_i, ev_inst_i = ensure_fname(f"__swapped_shadow_{other['idx1']}", 0)
            other['nm_i'] = ev_nm_i
            other['inst_i'] = ev_inst_i
            struct.pack_into('<ii', other['raw'], 12, ev_nm_i, ev_inst_i)

    # Rewire donor ProductAsset_*_TA export to target outer_index + object_name
    donor_pa_exp['out_i'] = cur_outer
    donor_pa_exp['nm_i'] = leaf_nm_i
    donor_pa_exp['inst_i'] = leaf_inst_i
    struct.pack_into('<iii', donor_pa_exp['raw'], 8, cur_outer, leaf_nm_i, leaf_inst_i)

    # Rewire donor _SF root Package export if target _SF root Package isn't already present
    sf_base, sf_inst = target_sf_fname
    sf_already_exists = any(
        cls_name(e['cls_i']).lower() == 'package'
        and e['out_i'] == 0
        and names_list[e['nm_i']].lower() == sf_base.lower()
        and e['inst_i'] == sf_inst
        for e in exps
    )
    if not sf_already_exists:
        for e in exps:
            if (
                cls_name(e['cls_i']).lower() == 'package'
                and e['out_i'] == 0
                and fmt_name(e['nm_i'], e['inst_i']).lower() == donor_pkg_stem.lower()
            ):
                sf_nm_i, sf_inst_i = ensure_fname(sf_base, sf_inst)
                e['nm_i'] = sf_nm_i
                e['inst_i'] = sf_inst_i
                struct.pack_into('<ii', e['raw'], 12, sf_nm_i, sf_inst_i)
                break

    new_nt = header[:nt_end] + bytes(extra_nt) + nt_tail
    new_import_rel = len(new_nt)
    new_export_rel = new_import_rel + len(imp_blob)
    exp_bytes_len = sum(len(e['raw']) for e in exps) + len(exp_tail)
    new_depends_rel = new_export_rel + exp_bytes_len
    header_delta = new_depends_rel - depends_off

    if header_delta != 0:
        for e in exps:
            so = struct.unpack_from('<q', e['raw'], 36)[0]
            if so > 0:
                struct.pack_into('<q', e['raw'], 36, so + header_delta)

    beyond = bytearray(header[depends_off:])
    if header_delta != 0:
        c_rel = compressed_chunks_offset - depends_off
        if 0 <= c_rel and c_rel + 4 <= len(beyond):
            num_chunks = struct.unpack_from('<i', beyond, c_rel)[0]
            for i in range(num_chunks):
                entry_off = c_rel + 4 + i * chunk_stride
                if entry_off + chunk_stride > len(beyond):
                    break
                u_off = struct.unpack_from('<q', beyond, entry_off)[0]
                struct.pack_into('<q', beyond, entry_off, u_off + header_delta)

    new_exp = b''.join(bytes(e['raw']) for e in exps) + exp_tail
    new_header = new_nt + imp_blob + new_exp + bytes(beyond)
    return (
        new_header,
        header_delta,
        len(names_list),
        len(exps),
        new_import_rel,
        new_export_rel,
        new_depends_rel,
    )


def swap_one_package(
    upk,
    source_path: Path,
    output_path: Path,
    key_source_path: Path,
    pairs: Sequence[Tuple[str, str]],
    options: SwapOptions,
    donor_asset_path: Optional[str] = None,
    target_asset_path: Optional[str] = None,
    donor_slot: str = "",
    color: Optional[str] = None,
) -> Tuple[Path, List[str]]:
    """
    Universal header-only swap (supports both AES-256-ECB and 0x800 AES-256-CTR packages):
      1. Decrypt AES header block from pristine donor file (ECB or CTR)
      2. Surgically rewire ExportTable + Core.Package outer chain (or fallback pairs for thumbnails)
      3. Expand total_header_size + compressed chunk offsets automatically if header exceeds padding
      4. Re-encrypt header (and CTR chunks if 0x800) with target file's key
      5. Backup original target (if not already backed up) and write output
    """
    log: List[str] = []

    pristine_source = get_pristine_path(source_path)
    if not pristine_source.exists():
        raise FileNotFoundError(f"Source package not found: {source_path}")

    # Read pristine donor bytes (and optionally bake color UObject edits) before touching output_path
    color_spec = None
    if color:
        import rl_color_injector
        color_spec = rl_color_injector.resolve_color_spec(color)

    if color_spec is not None:
        import tempfile
        import rl_color_injector
        if upk is None:
            upk = import_rl_upk_editor()
        d_key = _get_exact_key(source_path)
        if d_key is None:
            raise ValueError(f"No AES key found for {source_path.name}.")
        prov = upk.DecryptionProvider(None)
        prov.decryption_keys = [d_key]
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            # Copy pristine source to a clean .upk filename in case pristine_source ends with .upk.bak
            clean_src = td_path / source_path.name
            shutil.copy2(pristine_source, clean_src)
            dec_path = td_path / f"dec_{source_path.name}"
            upk.unpack_package(str(clean_src), str(dec_path), prov)
            parsed_pkg = upk.parse_decrypted_package(dec_path)
            colored_pkg, color_log = rl_color_injector.apply_color_to_package(
                parsed_pkg, donor_asset_path, donor_slot, color_spec
            )
            log.extend(color_log)
            rebuilt_src = td_path / f"colored_{source_path.name}"
            upk.build_reencrypted_package(
                clean_src, colored_pkg.file_bytes, prov, rebuilt_src, override_key=d_key
            )
            donor_data = rebuilt_src.read_bytes()
    else:
        donor_data = pristine_source.read_bytes()

    in_place = output_path.parent.resolve() == options.donor_dir.resolve()
    backup_path = output_path.with_suffix(output_path.suffix + ".bak")
    if output_path.exists() and not options.overwrite and not backup_path.exists():
        raise FileExistsError(f"Output already exists: {output_path}")

    if in_place and backup_path.exists():
        _ensure_pristine_target(output_path)
        log.append(f"Removed previous swap and restored pristine backup: {backup_path.name}")

    log.append(f"Source:  {pristine_source}")
    log.append(f"Output:  {output_path}")
    if donor_asset_path and target_asset_path:
        log.append(f"Rewire:  {donor_asset_path!r} -> {target_asset_path!r}")
    else:
        log.append("Pairs:   " + ", ".join(f"{o!r} -> {n!r}" for o, n in pairs))

    # ── Parse prefix ─────────────────────────────────────────────────────────
    pfx = _parse_upk_prefix(donor_data)
    chunk_stride = 36 if pfx.licensee_version >= 33 else 24

    name_off = pfx.name_offset
    enc_size = pfx.total_header_size - pfx.garbage_size - pfx.name_offset
    enc_aligned = (enc_size + 15) & ~15
    enc_block = donor_data[name_off:name_off + enc_aligned]

    # ── Find donor & output AES keys ─────────────────────────────────────────
    donor_key = _get_exact_key(source_path)
    if donor_key is None:
        raise ValueError(
            f"No AES key found for {source_path.name}. "
            f"Ensure keys_map.json is present next to the script.")
    log.append("Donor key: found")

    output_key = _get_exact_key(key_source_path) or donor_key
    log.append(f"Output key: {'target' if output_key != donor_key else 'donor (fallback)'}")

    # ── Decrypt, rewire, re-encrypt ──────────────────────────────────────────
    if pfx.ctr_nonce is not None:
        header_plain_full = _aes_ctr_crypt(donor_key, pfx.ctr_nonce, enc_block)
    else:
        header_plain_full = _aes_ecb_decrypt(donor_key, enc_block)
    header_plain = header_plain_full[:enc_size]

    import_rel = pfx.import_offset - pfx.name_offset
    export_rel = pfx.export_offset - pfx.name_offset
    depends_rel = pfx.depends_offset - pfx.name_offset

    if donor_asset_path and target_asset_path:
        donor_stem = source_path.stem
        target_stem = key_source_path.stem if key_source_path else output_path.stem
        target_chain_fnames, target_sf_fname = _extract_target_fnames_from_upk(
            key_source_path, output_key, target_asset_path, target_stem
        )
        (
            new_header,
            header_delta,
            new_name_count,
            new_export_count,
            new_import_rel,
            new_export_rel,
            new_depends_rel,
        ) = _rewire_header_surgical(
            header_plain,
            import_rel,
            export_rel,
            depends_rel,
            pfx.name_count,
            pfx.export_count,
            pfx.compressed_chunks_offset,
            donor_asset_path,
            donor_stem,
            target_chain_fnames,
            target_sf_fname,
            chunk_stride=chunk_stride,
        )
    else:
        new_header, header_delta = _apply_header_renames(
            header_plain, import_rel, export_rel, depends_rel,
            pfx.name_count, list(pairs),
            pfx.export_count, pfx.compressed_chunks_offset,
            chunk_stride=chunk_stride)
        new_name_count = pfx.name_count
        new_export_count = pfx.export_count
        new_import_rel = import_rel + header_delta
        new_export_rel = export_rel + header_delta
        new_depends_rel = depends_rel + header_delta

    new_header_mut = bytearray(new_header)
    new_enc_aligned = (len(new_header_mut) + 15) & ~15
    size_growth = new_enc_aligned - enc_aligned

    avail_gap = (pfx.total_header_size - pfx.name_offset) - enc_aligned
    if size_growth > avail_gap or header_delta > pfx.garbage_size:
        needed = max(size_growth - avail_gap, header_delta - pfx.garbage_size)
        extra_file_shift = (needed + 255) & ~15
        c_off_hdr = pfx.compressed_chunks_offset + header_delta
        if 0 <= c_off_hdr and c_off_hdr + 4 <= len(new_header_mut):
            num_chunks = struct.unpack_from('<i', new_header_mut, c_off_hdr)[0]
            for i in range(num_chunks):
                entry_off = c_off_hdr + 4 + i * chunk_stride
                if entry_off + chunk_stride > len(new_header_mut):
                    break
                comp_off = struct.unpack_from('<q', new_header_mut, entry_off + 12)[0]
                struct.pack_into('<q', new_header_mut, entry_off + 12, comp_off + extra_file_shift)
        log.append(f"Expanded header capacity by {extra_file_shift} bytes")
    else:
        extra_file_shift = 0

    new_header_padded = bytes(new_header_mut) + b'\x00' * (new_enc_aligned - len(new_header_mut))
    if pfx.ctr_nonce is not None:
        new_enc_block = _aes_ctr_crypt(output_key, pfx.ctr_nonce, new_header_padded)
    else:
        new_enc_block = _aes_ecb_encrypt(output_key, new_header_padded)

    # ── Splice header into donor file ─────────────────────────────────────────
    output = bytearray(donor_data)
    output[name_off:name_off + enc_aligned] = new_enc_block
    gap_adjust = extra_file_shift - size_growth
    gap_start = name_off + new_enc_aligned
    if gap_adjust < 0:
        del output[gap_start:gap_start + (-gap_adjust)]
    elif gap_adjust > 0:
        output[gap_start:gap_start] = b'\x00' * gap_adjust

    if (
        header_delta != 0
        or extra_file_shift != 0
        or new_name_count != pfx.name_count
        or new_export_count != pfx.export_count
    ):
        offs = _find_summary_offsets(bytes(output))
        if extra_file_shift != 0:
            _patch_i32(output, offs['total_header_size'], pfx.total_header_size + extra_file_shift)
        _patch_i32(output, offs['name_count'], new_name_count)
        _patch_i32(output, offs['export_count'], new_export_count)
        _patch_i32(output, offs['import_offset'], pfx.name_offset + new_import_rel)
        _patch_i32(output, offs['export_offset'], pfx.name_offset + new_export_rel)
        _patch_i32(output, offs['depends_offset'], pfx.name_offset + new_depends_rel)
        _patch_i32(output, offs['import_export_guids_offset'], pfx.name_offset + new_depends_rel)
        if offs['generation_count'] > 0:
            last_gen = offs['generation_entries'] + (offs['generation_count'] - 1) * 12
            _patch_i32(output, last_gen, new_export_count)
            _patch_i32(output, last_gen + 4, new_name_count)
        _patch_i32(output, pfx.meta_file_offset, pfx.garbage_size + extra_file_shift - header_delta)
        _patch_i32(output, pfx.meta_file_offset + 4, pfx.compressed_chunks_offset + header_delta)

    # ── Re-encrypt CTR compressed chunks if PackageFlags & 0x800 ──────────────
    if pfx.ctr_nonce is not None and output_key != donor_key:
        c_off = pfx.compressed_chunks_offset + header_delta
        if 0 <= c_off and c_off + 4 <= len(new_header_mut):
            num_chunks = struct.unpack_from('<i', new_header_mut, c_off)[0]
            for i in range(num_chunks):
                entry_off = c_off + 4 + i * 36
                if entry_off + 36 > len(new_header_mut):
                    break
                comp_off = struct.unpack_from('<q', new_header_mut, entry_off + 12)[0]
                comp_size = struct.unpack_from('<i', new_header_mut, entry_off + 20)[0]
                chunk_nonce = bytes(new_header_mut[entry_off + 24:entry_off + 36])
                if 0 < comp_size and 0 <= comp_off and comp_off + comp_size <= len(output):
                    chunk_plain = _aes_ctr_crypt(donor_key, chunk_nonce, bytes(output[comp_off:comp_off + comp_size]))
                    output[comp_off:comp_off + comp_size] = _aes_ctr_crypt(output_key, chunk_nonce, chunk_plain)

    # ── Backup original target, write output ─────────────────────────────────
    output_path.parent.mkdir(parents=True, exist_ok=True)
    if in_place and output_path.exists() and options.overwrite and not backup_path.exists():
        shutil.copy2(output_path, backup_path)
        log.append(f"Backup: {backup_path}")
    output_path.write_bytes(bytes(output))
    log.append(f"Swap complete: {len(output)} bytes written.")
    return output_path, log


def build_output(upk, donor_path: Path, target_key_path: Path, modified, provider, output_path: Path, was_encrypted: bool, log: List[str]) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    if was_encrypted and provider is not None:
        # Try exact key from Shift's CSV map first (no false positives)
        override_key = _get_exact_key(target_key_path)
        if override_key is not None:
            log.append(f"Exact key from map: {target_key_path.name}")
        elif target_key_path.exists() and hasattr(upk, "find_key_for_encrypted_upk"):
            try:
                override_key = upk.find_key_for_encrypted_upk(target_key_path, provider)
                log.append(f"Output key source:   {target_key_path}")
                log.append(f"Encrypting with key from target/original {target_key_path.name}: {base64.b64encode(override_key).decode()}")
            except Exception:
                log.append(f"WARN: target key not in database, falling back to donor key.")
        elif target_key_path.exists():
            log.append(f"Output key source exists but rl_upk_editor has no find_key_for_encrypted_upk: {target_key_path}")
        else:
            log.append(f"WARN: target key source missing, falling back to donor key: {target_key_path}")
        build_reencrypted_package_with_output_key(upk, donor_path, modified.file_bytes, provider, output_path, override_key) if override_key is not None else upk.build_reencrypted_package(donor_path, modified.file_bytes, provider, output_path)
        if override_key is not None:
            try:
                check_provider = upk.DecryptionProvider(None)
                check_provider.decryption_keys = [override_key]
                upk.find_valid_key(output_path, check_provider)
                log.append("Verified output decrypts with the target/original package key.")
            except Exception as exc:
                log.append(f"WARN: output key verification failed: {exc}")
        log.append("Saved encrypted/compressed output.")
    else:
        # Rocket League will refuse to load uncompressed files if the PKG_COOKED flag is missing.
        # rl_upk_editor strips this flag when decompressing, so we MUST restore it before saving!
        out_bytes = bytearray(modified.file_bytes)
        try:
            summary_offsets = upk._find_summary_offsets(out_bytes)
            import struct
            PKG_COOKED = 0x00000008
            flag_offset = summary_offsets['package_flags_offset']
            current_flags = struct.unpack_from('<I', out_bytes, flag_offset)[0]
            struct.pack_into('<I', out_bytes, flag_offset, current_flags | PKG_COOKED)
            log.append("Restored PKG_COOKED flag for unencrypted output.")
        except Exception as e:
            log.append(f"WARN: Failed to restore PKG_COOKED flag: {e}")
            
        output_path.write_bytes(out_bytes)
        log.append("Saved decrypted/decompressed output because input was not encrypted.")


def _normalize_slot(slot: str) -> str:
    return "Avatar Border" if slot == "PlayerAvatarBorder" else slot


def swap_asset(upk, target: Item, donor: Item, options: SwapOptions) -> Tuple[List[Path], List[str]]:
    if _normalize_slot(target.slot) != _normalize_slot(donor.slot):
        raise ValueError(f"Slot mismatch: target={target.slot!r}, donor={donor.slot!r}")
    key_dir = options.key_source_dir or options.donor_dir
    all_paths: List[Path] = []
    all_log: List[str] = []
    all_log.append(f"Target/replaced item: {target.label}")
    all_log.append(f"Donor/visual item:    {donor.label}")
    main_path, main_log = swap_one_package(
        upk,
        options.donor_dir / donor.asset_package,
        options.output_dir / target.asset_package,
        key_dir / target.asset_package,
        infer_name_pairs(target, donor),
        options,
        donor_asset_path=donor.asset_path,
        target_asset_path=target.asset_path,
        donor_slot=donor.slot,
        color=options.color,
    )
    all_paths.append(main_path)
    all_log.extend(main_log)

    if options.include_thumbnails:
        donor_thumb = options.donor_dir / donor.thumbnail_package
        target_thumb = options.output_dir / target.thumbnail_package
        key_thumb = key_dir / target.thumbnail_package
        if donor_thumb.exists() and key_thumb.exists():
            all_log.append("")
            all_log.append("Thumbnail/_T_SF pass:")
            try:
                thumb_path, thumb_log = swap_one_package(upk, donor_thumb, target_thumb, key_thumb, infer_thumbnail_pairs(target, donor), options)
                all_paths.append(thumb_path)
                all_log.extend(thumb_log)
            except Exception as exc:
                all_log.append(f"WARN: skipped thumbnail pass ({exc})")
        else:
            all_log.append(f"SKIP thumbnails: missing {donor_thumb if not donor_thumb.exists() else key_thumb}")
    else:
        all_log.append("SKIP thumbnails: disabled.")

    return all_paths, all_log


def cleanup_old_temp_files(directory: Path, logger: Optional[Callable[[str], None]] = None) -> None:
    import time
    if not directory.exists():
        return
    now = time.time()
    cutoff = 24 * 3600
    for file in directory.glob("*"):
        if file.name.endswith(("_decrypted.upk", "_decompressed.upk")):
            try:
                mtime = file.stat().st_mtime
                if now - mtime > cutoff:
                    file.unlink()
                    if logger:
                        logger(f"CLEANUP: Removed old temp file {file.name}")
            except Exception:
                pass

def swap_pfp(upk, pfp_upk_path: Path, options: SwapOptions) -> Tuple[List[Path], List[str]]:
    # This assumes the user provides a donor UPK that contains the custom PFP.
    # We'll swap it with the default avatar border or a known avatar package.
    target_package_name = "AvatarBorder_Default_SF.upk"
    target_export_path = "AvatarBorder_Default.AvatarBorder_Default"

    log: List[str] = []
    log.append(f"Custom PFP requested using donor: {pfp_upk_path}")

    return swap_export_only_path(upk, target_package_name, target_export_path, pfp_upk_path, target_export_path, options)


def swap_export_only_path(upk, target_pkg_name: str, target_export_path: str, donor_pkg_path: Path, donor_export_path: str, options: SwapOptions) -> Tuple[List[Path], List[str]]:
    log: List[str] = []
    target_pkg_path = options.output_dir / target_pkg_name
    key_dir = options.key_source_dir or options.donor_dir
    key_source_path = key_dir / target_pkg_name

    log.append(f"Replacing export {target_export_path} in {target_pkg_name} with {donor_export_path} from {donor_pkg_path}")

    backup_path = target_pkg_path.with_suffix(target_pkg_path.suffix + ".bak")
    if backup_path.exists():
        _ensure_pristine_target(target_pkg_path)

    temp_dir = script_dir() / "AssetSwapper_Decrypted"
    temp_dir.mkdir(exist_ok=True)

    _, target_package, target_provider, _, target_was_encrypted = resolve_with_optional_keys(upk, target_pkg_path, temp_dir, options.keys_path)
    _, donor_package, _, _, _ = resolve_with_optional_keys(upk, get_pristine_path(donor_pkg_path), temp_dir, options.keys_path)

    modified = upk.replace_export_with_donor_export(target_package, donor_package, target_export_path, donor_export_path)

    if target_pkg_path.exists() and options.overwrite and not backup_path.exists():
        shutil.copy2(target_pkg_path, backup_path)
        log.append(f"Backup written: {backup_path}")

    build_output(upk, target_pkg_path, key_source_path, modified, target_provider, target_pkg_path, target_was_encrypted, log)
    return [target_pkg_path], log


def swap_export_only(upk, target_pkg_name: str, target_export_path: str, donor_pkg_name: str, donor_export_path: str, options: SwapOptions) -> Tuple[List[Path], List[str]]:
    log: List[str] = []
    donor_pkg_path = get_pristine_path(options.donor_dir / donor_pkg_name)
    target_pkg_path = options.output_dir / target_pkg_name
    key_dir = options.key_source_dir or options.donor_dir
    key_source_path = key_dir / target_pkg_name

    log.append(f"Replacing export {target_export_path} in {target_pkg_name} with {donor_export_path} from {donor_pkg_name}")

    backup_path = target_pkg_path.with_suffix(target_pkg_path.suffix + ".bak")
    if backup_path.exists():
        _ensure_pristine_target(target_pkg_path)

    temp_dir = script_dir() / "AssetSwapper_Decrypted"
    temp_dir.mkdir(exist_ok=True)

    _, target_package, target_provider, _, target_was_encrypted = resolve_with_optional_keys(upk, target_pkg_path, temp_dir, options.keys_path)
    _, donor_package, _, _, _ = resolve_with_optional_keys(upk, donor_pkg_path, temp_dir, options.keys_path)

    modified = upk.replace_export_with_donor_export(target_package, donor_package, target_export_path, donor_export_path)

    if target_pkg_path.exists() and options.overwrite and not backup_path.exists():
        shutil.copy2(target_pkg_path, backup_path)
        log.append(f"Backup written: {backup_path}")

    build_output(upk, target_pkg_path, key_source_path, modified, target_provider, target_pkg_path, target_was_encrypted, log)
    return [target_pkg_path], log


def revert_item(target: Item, options: SwapOptions) -> Tuple[List[Path], List[str]]:
    src_dir = options.key_source_dir or options.donor_dir
    paths: List[Path] = []
    log: List[str] = []
    pairs = [(src_dir / target.asset_package, options.output_dir / target.asset_package)]
    if options.include_thumbnails:
        pairs.append((src_dir / target.thumbnail_package, options.output_dir / target.thumbnail_package))
    for src, dst in pairs:
        backup_path = dst.with_suffix(dst.suffix + ".bak")
        if backup_path.exists():
            if dst.exists():
                dst.unlink()
            shutil.copy2(backup_path, dst)
            backup_path.unlink()
            paths.append(dst)
            log.append(f"Reverted from backup: {backup_path} -> {dst}")
            continue
        if not src.exists():
            log.append(f"MISS: revert source not found: {src}")
            continue
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(get_pristine_path(src), dst)
        paths.append(dst)
        log.append(f"Reverted: {src} -> {dst}")
    return paths, log




# ── PNG → Custom PFP pipeline ─────────────────────────────────────────────────

_BULKDATA_TFC = 0x01  # stored in separate .tfc file


def _load_png_rgba(path: Path, w: int, h: int) -> List:
    try:
        from PIL import Image
    except ImportError:
        raise RuntimeError("Pillow is required for PNG input. Run: pip install Pillow")
    img = Image.open(str(path)).convert("RGBA").resize((w, h), Image.LANCZOS)
    return list(img.getdata())


def _dxt5_alpha_block(alphas: List[int]) -> bytes:
    a0, a1 = max(alphas), min(alphas)
    if a0 == a1:
        return bytes([a0, a1, 0, 0, 0, 0, 0, 0])
    table = [a0, a1] + [(a0 * (7 - i) + a1 * i) // 7 for i in range(1, 7)]
    indices = [min(range(8), key=lambda j, v=a: abs(table[j] - v)) for a in alphas]
    bits = 0
    for i in range(15, -1, -1):
        bits = (bits << 3) | (indices[i] & 7)
    return bytes([a0, a1]) + bits.to_bytes(6, 'little')


def _rgb565(r: int, g: int, b: int) -> int:
    return ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3)


def _from565(c: int) -> Tuple[int, int, int]:
    return (c >> 11) << 3, ((c >> 5) & 0x3F) << 2, (c & 0x1F) << 3


def _dxt1_color_block(rgbs: List[Tuple[int, int, int]]) -> bytes:
    c0v = _rgb565(max(p[0] for p in rgbs), max(p[1] for p in rgbs), max(p[2] for p in rgbs))
    c1v = _rgb565(min(p[0] for p in rgbs), min(p[1] for p in rgbs), min(p[2] for p in rgbs))
    if c0v == c1v:
        return struct.pack('<HHI', c0v, c1v, 0)
    if c0v < c1v:
        c0v, c1v = c1v, c0v
    c0, c1 = _from565(c0v), _from565(c1v)
    pal = [c0, c1,
           tuple((2*c0[i]+c1[i])//3 for i in range(3)),
           tuple((c0[i]+2*c1[i])//3 for i in range(3))]
    idx = 0
    for i, px in enumerate(rgbs):
        best = min(range(4), key=lambda j: sum((pal[j][k]-px[k])**2 for k in range(3)))
        idx |= best << (i * 2)
    return struct.pack('<HHI', c0v, c1v, idx)


def _compress_dxt5(pixels: List, w: int, h: int) -> bytes:
    pw, ph = (w + 3) & ~3, (h + 3) & ~3
    if pw != w or ph != h:
        pixels = [pixels[min(y, h-1)*w + min(x, w-1)] for y in range(ph) for x in range(pw)]
        w, h = pw, ph
    out = bytearray()
    for by in range(0, h, 4):
        for bx in range(0, w, 4):
            blk = [pixels[(by+dy)*w+(bx+dx)] for dy in range(4) for dx in range(4)]
            out += _dxt5_alpha_block([p[3] for p in blk])
            out += _dxt1_color_block([(p[0], p[1], p[2]) for p in blk])
    return bytes(out)


def _downsample(pixels: List, w: int, h: int) -> Tuple[List, int, int]:
    nw, nh = max(1, w >> 1), max(1, h >> 1)
    out = [tuple(sum(pixels[min(y*2+dy, h-1)*w+min(x*2+dx, w-1)][i] for dy in range(2) for dx in range(2)) // 4
                 for i in range(4))
           for y in range(nh) for x in range(nw)]
    return out, nw, nh


def _dxt5_mip_chain(pixels: List, w: int, h: int, n: int) -> List[bytes]:
    mips, cur, cw, ch = [], pixels, w, h
    for _ in range(n):
        mips.append(_compress_dxt5(cur, cw, ch))
        if cw <= 1 and ch <= 1:
            break
        cur, cw, ch = _downsample(cur, cw, ch)
    while len(mips) < n:
        mips.append(mips[-1])
    return mips


def _parse_texture2d_mips(serial: bytes, props_end: int) -> Tuple[int, List[dict], str]:
    """Returns (arr_start, mips, layout) where layout is 'A' or 'B'."""
    def is_pow2(n: int) -> bool:
        return n > 0 and (n & (n - 1)) == 0

    # Layout A: flags(4) + elem(4) + size_on_disk(4) + offset(8)
    # Layout B: flags(4) + elem(4) + offset(8) + size_on_disk(4)  ← standard UE3 source
    def try_at(start: int, layout: str):
        if start + 4 > len(serial):
            return None
        mc = struct.unpack_from('<i', serial, start)[0]
        if not (1 <= mc <= 16):
            return None
        pos = start + 4
        mips: List[dict] = []
        for _ in range(mc):
            if pos + 20 > len(serial):
                return None
            flags = struct.unpack_from('<I', serial, pos)[0]; pos += 4
            elem  = struct.unpack_from('<i', serial, pos)[0]; pos += 4
            if layout == 'A':
                disk = struct.unpack_from('<i', serial, pos)[0]; pos += 4
                off  = struct.unpack_from('<q', serial, pos)[0]; pos += 8
            else:
                off  = struct.unpack_from('<q', serial, pos)[0]; pos += 8
                disk = struct.unpack_from('<i', serial, pos)[0]; pos += 4
            is_tfc = bool(flags & _BULKDATA_TFC)
            data_start = pos
            if not is_tfc:
                read_len = disk if disk > 0 else (elem if elem > 0 else 0)
                if read_len < 0: read_len = 0
                if pos + read_len > len(serial): return None
                pos += read_len
            else:
                read_len = 0
            if pos + 8 > len(serial):
                return None
            mw = struct.unpack_from('<i', serial, pos)[0]; pos += 4
            mh = struct.unpack_from('<i', serial, pos)[0]; pos += 4
            if not (is_pow2(mw) and is_pow2(mh) and 0 < mw <= 4096 and 0 < mh <= 4096):
                return None
            mips.append(dict(flags=flags, elem_count=elem, size_on_disk=disk,
                             bulk_offset=off, data_start=data_start,
                             data_len=read_len, w=mw, h=mh, is_tfc=is_tfc))
        return mips

    scan_end = min(props_end + 256, len(serial) - 4)
    for start in range(props_end, scan_end, 4):
        for layout in ('B', 'A'):
            result = try_at(start, layout)
            if result:
                return start, result, layout

    raise ValueError(
        f"Cannot locate Texture2D mip array near offset {props_end} "
        f"(serial length {len(serial)})"
    )


def _rebuild_texture2d_serial(serial: bytes, arr_start: int, mips: List[dict], new_inline: List[bytes], layout: str = 'B') -> bytes:
    prefix = serial[:arr_start + 4]  # everything up to and including mip count
    inline_iter = iter(new_inline)
    body = bytearray()
    last_end = arr_start + 4
    for mip in mips:
        hdr_start = mip['data_start'] - 20
        if mip['is_tfc']:
            body += serial[hdr_start: hdr_start + 20]
        else:
            nd = next(inline_iter)
            body += struct.pack('<I', mip['flags'])
            body += struct.pack('<i', len(nd))
            if layout == 'A':
                body += struct.pack('<i', len(nd))
                body += struct.pack('<q', mip['bulk_offset'])
            else:
                body += struct.pack('<q', mip['bulk_offset'])
                body += struct.pack('<i', len(nd))
            body += nd
        body += struct.pack('<i', mip['w'])
        body += struct.pack('<i', mip['h'])
        last_end = mip['data_start'] + mip['data_len'] + 8
    return prefix + bytes(body) + serial[last_end:]


def _read_upk_texture_props(pkg, serial: bytes) -> Tuple[int, int, str]:
    """
    Read SizeX, SizeY, and Format from a cooked RL Texture2D serial.
    Properties start at byte 4 (byte 0 is a 4-byte cooked strip-flag sentinel).
    """
    def name_idx(name: str) -> int:
        indices, _ = find_name_indices(pkg, name)
        return indices[0] if indices else -1

    size_x_idx   = name_idx('SizeX')
    size_y_idx   = name_idx('SizeY')
    int_prop_idx = name_idx('IntProperty')
    none_idx     = name_idx('None')

    width = height = 0
    pos = 4  # skip 4-byte sentinel at offset 0
    for _ in range(100):
        if pos + 8 > len(serial):
            break
        ni = struct.unpack_from('<i', serial, pos)[0]
        if ni == none_idx or ni < 0:
            break
        ti         = struct.unpack_from('<i', serial, pos + 8)[0] if pos + 12 <= len(serial) else -1
        prop_size  = struct.unpack_from('<i', serial, pos + 16)[0] if pos + 20 <= len(serial) else -1
        if prop_size < 0 or prop_size > 100000:
            break
        if ti == int_prop_idx and prop_size == 4 and pos + 28 <= len(serial):
            value = struct.unpack_from('<i', serial, pos + 24)[0]
            if ni == size_x_idx:
                width = value
            elif ni == size_y_idx:
                height = value
        pos += 24 + prop_size

    # Detect pixel format: scan first 600 bytes for a known format name index
    fmt = 'PF_A8R8G8B8'
    for fmt_name in ('PF_DXT5', 'PF_DXT1'):
        idx = name_idx(fmt_name)
        if idx >= 0:
            for i in range(0, min(len(serial) - 4, 600), 4):
                if struct.unpack_from('<i', serial, i)[0] == idx:
                    fmt = fmt_name
                    break
        if fmt != 'PF_A8R8G8B8':
            break

    return width, height, fmt





def _inplace_zlib_patch(upk, png_path: Path, options: SwapOptions, target_pkg: str, target_chunk_idx: int, target_width: int, target_height: int, img_size, img_offset, magic_size: int) -> Tuple[List[Path], List[str]]:
    log: List[str] = []
    log.append(f"Custom Image from PNG: {png_path}")
    
    if not png_path.exists():
        raise FileNotFoundError(f"PNG not found: {png_path}")

    import shutil, struct, zlib
    from PIL import Image
    
    original_img = Image.open(str(png_path)).convert("RGBA")
    
    def _get_bgra_bytes(im):
        rgba = im.tobytes()
        bgra = bytearray(len(rgba))
        for i in range(0, len(rgba), 4):
            bgra[i]   = rgba[i + 2]
            bgra[i+1] = rgba[i + 1]
            bgra[i+2] = rgba[i]
            bgra[i+3] = rgba[i + 3]
        return bytes(bgra)
        
    source_path = options.donor_dir / target_pkg
    target_path = options.output_dir / target_pkg
    pristine_source = get_pristine_path(source_path)

    if not pristine_source.exists():
        raise FileNotFoundError(f"Source UPK not found: {source_path}")

    in_place = source_path.resolve() == target_path.resolve()
    original_bytes = bytearray(pristine_source.read_bytes())

    if in_place:
        _ensure_pristine_target(target_path)

    pfx = _parse_upk_prefix(bytes(original_bytes))
    pkg_key = _get_exact_key(source_path) if pfx.ctr_nonce is not None else None
    ctr_chunk_nonce = None

    if pfx.ctr_nonce is not None and pkg_key is not None:
        enc_size = pfx.total_header_size - pfx.garbage_size - pfx.name_offset
        enc_aligned = (enc_size + 15) & ~15
        hdr_plain = _aes_ctr_crypt(pkg_key, pfx.ctr_nonce, bytes(original_bytes[pfx.name_offset:pfx.name_offset + enc_aligned]))
        c_idx = max(0, target_chunk_idx - 1)
        entry_off = pfx.compressed_chunks_offset + 4 + c_idx * 36
        start = struct.unpack_from('<q', hdr_plain, entry_off + 12)[0]
        full_chunk_size = struct.unpack_from('<i', hdr_plain, entry_off + 20)[0]
        ctr_chunk_nonce = hdr_plain[entry_off + 24:entry_off + 36]
        payload = bytearray(_aes_ctr_crypt(pkg_key, ctr_chunk_nonce, bytes(original_bytes[start:start + full_chunk_size])))
        c_magic, c_block_size, c_comp, c_uncomp = struct.unpack_from('<Iiii', payload, 0)
        block_offset = 16
        blocks = []
        while block_offset < 16 + 8 * ((c_uncomp + c_block_size - 1) // c_block_size):
            bc, bu = struct.unpack_from('<ii', payload, block_offset)
            blocks.append((bc, bu))
            block_offset += 8
    else:
        chunk_offsets = []
        idx = 0
        while True:
            idx = original_bytes.find(b'\xc1\x83\x2a\x9e', idx)
            if idx == -1: break
            chunk_offsets.append(idx)
            idx += 4
            
        if len(chunk_offsets) <= target_chunk_idx:
            raise ValueError(f"Could not find enough chunks in {target_pkg}")
            
        start = chunk_offsets[target_chunk_idx]
        c_magic, c_block_size, c_comp, c_uncomp = struct.unpack_from('<Iiii', original_bytes, start)
        
        block_offset = 16
        blocks = []
        while block_offset < 16 + 8 * ((c_uncomp + c_block_size - 1) // c_block_size):
            bc, bu = struct.unpack_from('<ii', original_bytes, start + block_offset)
            blocks.append((bc, bu))
            block_offset += 8
            
        full_chunk_size = block_offset + sum(bc for bc, bu in blocks)
        payload = original_bytes[start : start + full_chunk_size]
    
    uncompressed_blocks = []
    data_offset = block_offset
    for bc, bu in blocks:
        uncompressed_blocks.append(bytearray(zlib.decompress(payload[data_offset : data_offset + bc])))
        data_offset += bc
        
    uncompressed = bytearray()
    for b in uncompressed_blocks: uncompressed += b
        
    magic_bytes = struct.pack('<II', magic_size, magic_size)
    find_idx = uncompressed.find(magic_bytes)
    if find_idx == -1:
        magic_bytes = struct.pack('<I', magic_size)
        find_idx = uncompressed.find(magic_bytes)
        if find_idx == -1:
            raise ValueError(f"Could not find BulkData header for {magic_size} bytes.")
            
    pixel_start = find_idx + 8
    pixel_end = pixel_start + magic_size

    if img_size is not None:
        scaled = original_img.resize((img_size, img_size), Image.LANCZOS)
        img = Image.new("RGBA", (target_width, target_height), (0, 0, 0, 0))
        img.paste(scaled, (img_offset, img_offset))
        
        # Extract the original texture to use as an overlay border
        orig_bgra = uncompressed[pixel_start : pixel_end]
        rgba = bytearray(len(orig_bgra))
        for i in range(0, len(orig_bgra), 4):
            rgba[i]   = orig_bgra[i+2]
            rgba[i+1] = orig_bgra[i+1]
            rgba[i+2] = orig_bgra[i]
            rgba[i+3] = orig_bgra[i+3]
        border_img = Image.frombytes("RGBA", (target_width, target_height), bytes(rgba))
        
        # Paste the original border on top of the custom image
        img.paste(border_img, (0, 0), border_img)
    elif target_height == 100:  # THIS IS A BANNER
        scaled = original_img.resize((target_width, 90), Image.LANCZOS)
        img = Image.new("RGBA", (target_width, target_height), (0, 0, 0, 0))
        img.paste(scaled, (0, 5))
    else:
        img = original_img.resize((target_width, target_height), Image.LANCZOS)

    best_bytes = _get_bgra_bytes(img)
    
    def try_compress(image_bytes):
        temp_uncomp = bytearray(uncompressed)
        temp_uncomp[pixel_start:pixel_end] = image_bytes
        new_zlibs = []
        offset = 0
        data_offset = block_offset
        for bc, bu in blocks:
            block_start = offset
            block_end = offset + bu
            
            if block_end > pixel_start and block_start < pixel_end:
                b_data = temp_uncomp[block_start : block_end]
                nz = zlib.compress(b_data, 9)
                if len(nz) > bc:
                    return None
                new_zlibs.append(nz)
            else:
                orig_nz = payload[data_offset : data_offset + bc]
                new_zlibs.append(orig_nz)
                
            offset += bu
            data_offset += bc
        return new_zlibs
        
    new_zlibs = try_compress(best_bytes)
    
    if new_zlibs is None:
        log.append("Image is too complex, compressing with quantization...")
        for colors in [256, 128, 64, 32, 16, 8, 4]:
            q = img.quantize(colors=colors).convert("RGBA")
            q_bytes = _get_bgra_bytes(q)
            new_zlibs = try_compress(q_bytes)
            if new_zlibs is not None:
                log.append(f"Quantized to {colors} colors.")
                break
        else:
            raise ValueError("Image still too large after extreme quantization!")
            
    # Rebuild the payload with exactly the same chunk header and block sizes
    new_payload = bytearray(payload[:block_offset])
    for i, nz in enumerate(new_zlibs):
        bc, bu = blocks[i]
        padded_nz = nz + b'\x00' * (bc - len(nz))
        new_payload += padded_nz
        
    if ctr_chunk_nonce is not None and pkg_key is not None:
        original_bytes[start : start + full_chunk_size] = _aes_ctr_crypt(pkg_key, ctr_chunk_nonce, bytes(new_payload))
    else:
        original_bytes[start : start + full_chunk_size] = new_payload
    
    target_path.parent.mkdir(parents=True, exist_ok=True)
    if in_place and target_path.exists() and options.overwrite:
        bak = target_path.with_suffix(target_path.suffix + ".bak")
        if not bak.exists():
            shutil.copy2(target_path, bak)
        
    target_path.write_bytes(original_bytes)
    log.append(f"Successfully patched {target_pkg} in-place!")
    return [target_path], log


def swap_pfp_from_png(upk, png_path: Path, target_pkg: str, options: 'SwapOptions') -> Tuple[List[Path], List[str]]:
    return _inplace_zlib_patch(
        upk=upk,
        png_path=png_path,
        options=options,
        target_pkg=target_pkg,
        target_chunk_idx=1,
        target_width=120,
        target_height=120,
        img_size=84,
        img_offset=18,
        magic_size=57600
    )


def swap_banner_from_png(upk, png_path: Path, target_pkg: str, options: 'SwapOptions') -> Tuple[List[Path], List[str]]:
    return _inplace_zlib_patch(
        upk=upk,
        png_path=png_path,
        options=options,
        target_pkg=target_pkg,
        target_chunk_idx=1,
        target_width=420,
        target_height=100,
        img_size=None,
        img_offset=0,
        magic_size=168000,
    )
