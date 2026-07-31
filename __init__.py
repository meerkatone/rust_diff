"""
Binary Diffing Plugin for Binary Ninja

Python is only the frontend here: it extracts per-function features from the
BinaryViews and hands them as JSON to the Rust engine (librust_diff), which
performs all matching (WL graph hashes, MD-Index, small-primes-product,
call-graph propagation, fuzzy matching) and returns a JSON DiffResult.
"""
import binaryninja as bn
from binaryninja import BackgroundTaskThread, PluginCommand, BinaryView
from binaryninja import log_info, log_error
from binaryninja import get_open_filename_input
import ctypes
import hashlib
import json
import os
import platform
import sys
import time

# Add the plugin directory to sys.path for imports
plugin_dir = os.path.dirname(os.path.abspath(__file__))
if plugin_dir not in sys.path:
    sys.path.insert(0, plugin_dir)

try:
    from diff_results_ui import show_diff_results
    HAS_GUI = True
    log_info("Qt GUI components loaded successfully")
except ImportError as e:
    log_error(f"Failed to import GUI components: {e}")
    log_info("To enable Qt GUI features, install PySide6 or PySide2:")
    log_info("  pip install PySide6")
    log_info("  or run: python install_pyside.py")
    HAS_GUI = False


# ---------------------------------------------------------------------------
# Rust engine bridge
# ---------------------------------------------------------------------------

_RUST_LIB = None


def _rust_lib_path():
    names = {
        "Darwin": "librust_diff.dylib",
        "Linux": "librust_diff.so",
        "Windows": "rust_diff.dll",
    }
    name = names.get(platform.system(), "librust_diff.so")
    return os.path.join(plugin_dir, "target", "release", name)


def load_rust_engine():
    """Load (once) the Rust diffing engine via ctypes. Returns None on failure."""
    global _RUST_LIB
    if _RUST_LIB is not None:
        return _RUST_LIB

    path = _rust_lib_path()
    if not os.path.exists(path):
        log_error(f"Rust diff engine not found at {path}")
        log_error("Build it with: cargo build --release (in the plugin directory)")
        return None

    try:
        lib = ctypes.CDLL(path)
        lib.rust_diff_diff_json.argtypes = [ctypes.c_char_p, ctypes.c_char_p]
        # void* (not c_char_p) so we keep the original pointer to free it
        lib.rust_diff_diff_json.restype = ctypes.c_void_p
        lib.rust_diff_free_string.argtypes = [ctypes.c_void_p]
        lib.rust_diff_free_string.restype = None
        # Semantic IL diff (single request blob in, IlDiff JSON out).
        lib.rust_diff_il_diff_json.argtypes = [ctypes.c_char_p]
        lib.rust_diff_il_diff_json.restype = ctypes.c_void_p
    except OSError as e:
        log_error(f"Failed to load Rust diff engine: {e}")
        return None

    _RUST_LIB = lib
    return lib


def rust_diff(functions_a, functions_b):
    """Run the Rust matcher on two extracted function lists. Returns the
    DiffResult dict, or None on failure."""
    lib = load_rust_engine()
    if lib is None:
        return None

    json_a = json.dumps(functions_a).encode("utf-8")
    json_b = json.dumps(functions_b).encode("utf-8")

    ptr = lib.rust_diff_diff_json(json_a, json_b)
    if not ptr:
        log_error("Rust diff engine returned an error (see log for details)")
        return None
    try:
        result_json = ctypes.cast(ptr, ctypes.c_char_p).value.decode("utf-8")
    finally:
        lib.rust_diff_free_string(ptr)

    return json.loads(result_json)


# ---------------------------------------------------------------------------
# IL-aware (semantic) diff
# ---------------------------------------------------------------------------

# Map Binary Ninja IL level names to the function attribute that yields the IL.
_IL_ACCESSORS = {
    "LLIL": "llil",
    "MLIL": "mlil",
    "HLIL": "hlil",
}


def extract_il_function(func, level):
    """Build an IL token stream (`il::IlFunction` JSON shape) for one function
    at the given level ("LLIL"/"MLIL"/"HLIL"). Each rendered IL line becomes a
    list of typed tokens (kind = Binary Ninja token type name) plus the
    rendered text. Returns None if the IL is unavailable."""
    accessor = _IL_ACCESSORS.get(level)
    if accessor is None:
        return None
    try:
        il = getattr(func, accessor, None)
    except Exception:
        il = None
    if not il:
        return None

    lines = []
    try:
        for instr in il.instructions:
            tokens = []
            text_parts = []
            for tok in getattr(instr, "tokens", []) or []:
                # InstructionTextToken.type is an enum; its name is the kind.
                kind = getattr(getattr(tok, "type", None), "name", "") or ""
                text = tok.text
                tokens.append({"kind": kind, "text": text})
                text_parts.append(text)
            lines.append({"tokens": tokens, "text": "".join(text_parts)})
    except Exception as e:
        log_error(f"Error extracting {level} for {func.name}: {e}")
        return None

    return {"level": level, "lines": lines}


def il_diff(il_a, il_b, rename_map=None, addr_map=None):
    """Run the Rust semantic IL diff. `il_a`/`il_b` are `extract_il_function`
    results; `rename_map` maps callee-symbol-in-A -> callee-symbol-in-B and
    `addr_map` maps callee-code-address-in-A -> -in-B for matched callees, so
    renamed/relocated-but-matched calls read as cosmetic. Returns the IlDiff
    dict, or None on failure."""
    lib = load_rust_engine()
    if lib is None or il_a is None or il_b is None:
        return None

    request = {"a": il_a, "b": il_b, "rename_map": rename_map or {}, "addr_map": addr_map or {}}
    request_json = json.dumps(request).encode("utf-8")

    ptr = lib.rust_diff_il_diff_json(request_json)
    if not ptr:
        log_error("Rust IL diff returned an error (see log for details)")
        return None
    try:
        result_json = ctypes.cast(ptr, ctypes.c_char_p).value.decode("utf-8")
    finally:
        lib.rust_diff_free_string(ptr)

    return json.loads(result_json)


# ---------------------------------------------------------------------------
# Feature extraction
# ---------------------------------------------------------------------------

def _extract_basic_block(bb):
    """Extract instructions, edges and a mnemonic hash from one basic block."""
    instructions = []
    mnemonics = []
    addr = bb.start
    for tokens, length in bb:
        mnemonic = tokens[0].text.strip() if tokens else ""
        operands = [
            t.text.strip()
            for t in tokens[1:]
            if t.text.strip() and t.text.strip() != ","
        ]
        instructions.append({
            "address": addr,
            "mnemonic": mnemonic,
            "operands": operands,
            "bytes": [],
            "length": length,
        })
        mnemonics.append(mnemonic)
        addr += length

    # Deterministic across sessions, unlike Python's salted hash().
    mnemonic_hash = hashlib.sha256(" ".join(mnemonics).encode("utf-8")).hexdigest()[:16]

    return {
        "address": bb.start,
        "size": bb.length,
        "instructions": instructions,
        "edges": sorted({e.target.start for e in bb.outgoing_edges}),
        "mnemonic_hash": mnemonic_hash,
        "instruction_count": len(instructions),
    }


def _extract_function(func):
    """Extract one function into the Rust FunctionInfo JSON shape."""
    basic_blocks = []
    for bb in func.basic_blocks:
        try:
            basic_blocks.append(_extract_basic_block(bb))
        except Exception as e:
            log_error(f"Error extracting block at 0x{bb.start:x} in {func.name}: {e}")

    instructions = [i for b in basic_blocks for i in b["instructions"]]
    edge_count = sum(len(b["edges"]) for b in basic_blocks)
    complexity = max(1, edge_count - len(basic_blocks) + 2)

    try:
        callees = sorted({c.start for c in func.callees})
    except Exception:
        callees = []
    try:
        callers = sorted({c.start for c in func.callers})
    except Exception:
        callers = []

    size = func.total_bytes
    if not size:
        size = max(1, func.highest_address - func.lowest_address)

    return {
        "name": func.name,
        "address": func.start,
        "size": size,
        "basic_blocks": basic_blocks,
        "instructions": instructions,
        "cyclomatic_complexity": complexity,
        # Structure hashes are computed in Rust (preprocess_functions) so
        # they are deterministic and frontend-independent.
        "call_graph_hash": "",
        "cfg_hash": "",
        "instruction_count": len(instructions),
        "call_count": len(callees),
        "callees": callees,
        "callers": callers,
    }


# ---------------------------------------------------------------------------
# On-demand IL refinement
# ---------------------------------------------------------------------------
# The bulk matcher sees only disassembly features. After it runs, we extract IL
# for a *small* candidate set — weak existing matches and a few size-near
# unmatched pairs — and use semantic IL similarity to confirm/re-rank and to
# rescue matches the disassembly heuristics missed. Bounded so large binaries
# stay responsive.

# Match types that are already near ground truth and not worth re-checking.
_STRONG_MATCH_TYPES = {"Exact", "Name", "MdIndex", "SmallPrimes"}
_REFINE_CONF_BELOW = 0.85   # only re-check matches at/under this confidence
_REFINE_MAX = 400           # cap weak matches to refine
_RESCUE_MAX_A = 300         # cap unmatched-A functions to attempt rescuing
_RESCUE_TOPK = 6            # size-nearest B candidates to score per A
_RESCUE_IL_THRESHOLD = 0.6  # min IL similarity to accept a rescued match
_REFINE_MIN_INSTRS = 5      # below this, IL carries too little signal
_REFINE_IL_LEVEL = "MLIL"   # fast, more stable across builds than LLIL


def _confidence_for_match(match_type, similarity):
    """Mirror the Rust confidence calibration after Python-side refinement."""
    s = max(0.0, min(1.0, float(similarity)))
    if match_type == "Exact":
        return 1.0
    if match_type == "Name":
        return 0.90 + 0.10 * s
    if match_type == "MdIndex":
        return 0.80 + 0.10 * s
    if match_type == "SmallPrimes":
        return 0.75 + 0.10 * s
    if match_type == "Structural":
        return 0.70 + 0.15 * s
    if match_type == "CallGraph":
        return 0.50 + 0.35 * s
    if match_type == "IL":
        return 0.45 + 0.45 * s
    if match_type == "Manual":
        return 1.0
    return 0.35 + 0.45 * s


def _callee_maps(bv_a, bv_b, match_index, address_a):
    """Build semantic rename/relocation maps for one A-side function."""
    rename_map, addr_map = {}, {}
    try:
        func_a = bv_a.get_function_at(address_a)
    except Exception:
        return rename_map, addr_map
    if func_a is None:
        return rename_map, addr_map
    try:
        callees = list(func_a.callees)
    except Exception:
        return rename_map, addr_map
    for callee_a in callees:
        address_b = match_index.get(callee_a.start)
        if address_b is None:
            continue
        try:
            callee_b = bv_b.get_function_at(address_b)
        except Exception:
            continue
        if callee_b is None:
            continue
        if callee_a.name and callee_b.name and callee_a.name != callee_b.name:
            rename_map[callee_a.name] = callee_b.name
        if callee_a.start != callee_b.start:
            addr_map[f"0x{callee_a.start:x}"] = f"0x{callee_b.start:x}"
    return rename_map, addr_map


def _recompute_result_metrics(result):
    matches = result.get("matched_functions", [])
    total_a = len(matches) + len(result.get("unmatched_functions_a", []))
    total_b = len(matches) + len(result.get("unmatched_functions_b", []))
    mean = (sum(m.get("similarity", 0.0) for m in matches) / len(matches)
            if matches else (1.0 if total_a == 0 and total_b == 0 else 0.0))
    denominator = max(total_a, total_b)
    coverage = len(matches) / denominator if denominator else 1.0
    result["matched_similarity_score"] = round(mean, 6)
    result["match_coverage"] = round(coverage, 6)
    result["similarity_score"] = round(mean * coverage, 6)


def _il_for(bv, address, cache):
    """Extract (and cache) the IL token stream for one function address."""
    if address in cache:
        return cache[address]
    func = bv.get_function_at(address)
    il = extract_il_function(func, _REFINE_IL_LEVEL) if func else None
    cache[address] = il
    return il


def refine_matches_with_il(bv_a, bv_b, result, progress=None, cancelled=None):
    """Confirm/re-rank weak matches and rescue missed ones using semantic IL
    similarity. Mutates `result` in place; safe to skip on any failure."""
    if load_rust_engine() is None:
        return

    cache_a, cache_b = {}, {}
    matches = result.get("matched_functions", [])
    match_index = {
        m.get("function_a", {}).get("address"): m.get("function_b", {}).get("address")
        for m in matches
        if m.get("function_a", {}).get("address") is not None
        and m.get("function_b", {}).get("address") is not None
    }
    callee_map_cache = {}

    def maps_for(address):
        if address not in callee_map_cache:
            callee_map_cache[address] = _callee_maps(
                bv_a, bv_b, match_index, address)
        return callee_map_cache[address]

    # 1. Re-check weak matches: blend IL similarity into the score.
    refined = 0
    for m in matches:
        if cancelled is not None and cancelled():
            return
        if refined >= _REFINE_MAX:
            break
        if m.get("match_type") in _STRONG_MATCH_TYPES and m.get("confidence", 0) >= _REFINE_CONF_BELOW:
            continue
        fa, fb = m.get("function_a", {}), m.get("function_b", {})
        if min(fa.get("instruction_count", 0), fb.get("instruction_count", 0)) < _REFINE_MIN_INSTRS:
            continue
        il_a = _il_for(bv_a, fa.get("address"), cache_a)
        il_b = _il_for(bv_b, fb.get("address"), cache_b)
        rename_map, addr_map = maps_for(fa.get("address"))
        d = il_diff(il_a, il_b, rename_map, addr_map)
        if d is None:
            continue
        il_sim = d.get("similarity", 0.0)
        m["il_similarity"] = round(il_sim, 4)
        # Blend so IL evidence can both raise and lower a weak score.
        m["similarity"] = round(0.5 * m.get("similarity", 0.0) + 0.5 * il_sim, 4)
        m["confidence"] = round(
            _confidence_for_match(m.get("match_type"), m["similarity"]), 4)
        refined += 1

    # 2. Rescue: score each unmatched-A against size-near unmatched-B.
    unmatched_a = result.get("unmatched_functions_a", [])
    unmatched_b = result.get("unmatched_functions_b", [])
    b_pool = sorted(
        (fb for fb in unmatched_b if fb.get("instruction_count", 0) >= _REFINE_MIN_INSTRS),
        key=lambda f: f.get("instruction_count", 0),
    )
    b_counts = [f.get("instruction_count", 0) for f in b_pool]
    used_b_addrs = set()
    rescued = []

    import bisect
    for ai, fa in enumerate(unmatched_a[:_RESCUE_MAX_A]):
        if cancelled is not None and cancelled():
            return
        ca = fa.get("instruction_count", 0)
        if ca < _REFINE_MIN_INSTRS or not b_pool:
            continue
        # Window of the K size-nearest B candidates around ca.
        lo = bisect.bisect_left(b_counts, ca)
        cand_idx = sorted(range(max(0, lo - _RESCUE_TOPK), min(len(b_pool), lo + _RESCUE_TOPK)),
                          key=lambda j: abs(b_counts[j] - ca))[:_RESCUE_TOPK]
        il_a = _il_for(bv_a, fa.get("address"), cache_a)
        if il_a is None:
            continue
        best = None
        for j in cand_idx:
            if cancelled is not None and cancelled():
                return
            fb = b_pool[j]
            if fb.get("address") in used_b_addrs:
                continue
            il_b = _il_for(bv_b, fb.get("address"), cache_b)
            rename_map, addr_map = maps_for(fa.get("address"))
            d = il_diff(il_a, il_b, rename_map, addr_map)
            if d is None:
                continue
            il_sim = d.get("similarity", 0.0)
            if il_sim >= _RESCUE_IL_THRESHOLD and (best is None or il_sim > best[1]):
                best = (fb, il_sim)
        if best is not None:
            fb, il_sim = best
            used_b_addrs.add(fb.get("address"))
            rescued.append((ai, fb, il_sim))

    # Apply rescues: add matches, drop the paired functions from unmatched lists.
    if rescued:
        rescued_a_idx = {ai for ai, _, _ in rescued}
        for ai, fb, il_sim in rescued:
            fa = unmatched_a[ai]
            matches.append({
                "function_a": fa,
                "function_b": fb,
                "similarity": round(il_sim, 4),
                "confidence": round(_confidence_for_match("IL", il_sim), 4),
                "match_type": "IL",
                "il_similarity": round(il_sim, 4),
                "details": {},
            })
        result["unmatched_functions_a"] = [
            f for i, f in enumerate(unmatched_a) if i not in rescued_a_idx]
        result["unmatched_functions_b"] = [
            f for f in unmatched_b if f.get("address") not in used_b_addrs]

    # Blended similarities and rescued matches changed all aggregate metrics.
    _recompute_result_metrics(result)

    if progress is not None:
        progress(refined, len(rescued))


class BinaryDiffTask(BackgroundTaskThread):
    """Background task: extract features from both binaries and run the Rust engine."""

    def __init__(self, bv1: BinaryView, bv2: BinaryView, on_complete=None):
        super().__init__("Binary Diffing", True)
        self.bv1 = bv1
        self.bv2 = bv2
        self.result = None
        self.on_complete = on_complete

    def _extract_binary_features(self, bv):
        functions = []
        function_list = list(bv.functions)
        total = len(function_list)
        log_info(f"Extracting features from {total} functions in {bv.file.filename}")
        for i, func in enumerate(function_list):
            if self.cancelled:
                return None
            if i % 100 == 0:
                self.progress = f"Extracting {bv.file.filename}: {i}/{total}"
            try:
                functions.append(_extract_function(func))
            except Exception as e:
                log_error(f"Error extracting function {func.name}: {e}")
        return functions

    def run(self):
        started = time.monotonic()
        try:
            self.progress = "Extracting features from first binary..."
            features1 = self._extract_binary_features(self.bv1)
            if features1 is None or self.cancelled:
                return

            self.progress = "Extracting features from second binary..."
            features2 = self._extract_binary_features(self.bv2)
            if features2 is None or self.cancelled:
                return

            self.progress = f"Matching {len(features1)} x {len(features2)} functions (Rust engine)..."
            result = rust_diff(features1, features2)
            if result is None or self.cancelled:
                return

            result["binary_a_name"] = self.bv1.file.filename
            result["binary_b_name"] = self.bv2.file.filename

            # On-demand IL refinement pass (bounded; best-effort).
            if not self.cancelled:
                self.progress = "Refining matches with IL similarity..."
                try:
                    def _report(refined, rescued):
                        log_info(f"IL refinement: re-checked {refined} weak match(es), "
                                 f"rescued {rescued} new match(es)")
                    refine_matches_with_il(
                        self.bv1, self.bv2, result, progress=_report,
                        cancelled=lambda: self.cancelled)
                except Exception as e:
                    log_error(f"IL refinement pass failed (continuing): {e}")

            self.result = result
            result["analysis_time"] = time.monotonic() - started

            log_info(
                f"Binary diff completed in {result.get('analysis_time', 0):.2f}s: "
                f"{len(result.get('matched_functions', []))} matches, "
                f"{len(result.get('unmatched_functions_a', []))} unmatched in A, "
                f"{len(result.get('unmatched_functions_b', []))} unmatched in B"
            )

            if not self.cancelled and self.on_complete is not None:
                self.on_complete(self.result)
        except Exception as e:
            log_error(f"Error during binary diffing: {e}")


def _log_summary(result):
    matches = sorted(
        result.get("matched_functions", []),
        key=lambda m: (-m.get("confidence", 0), -m.get("similarity", 0)),
    )
    log_info("=" * 60)
    log_info(f"BINARY DIFF RESULTS - {len(matches)} MATCHES FOUND")
    log_info(f"Binary 1: {result.get('binary_a_name')}")
    log_info(f"Binary 2: {result.get('binary_b_name')}")
    log_info(f"Overall similarity: {result.get('similarity_score', 0):.4f}")
    log_info(f"Matched-pair similarity: {result.get('matched_similarity_score', 0):.4f}")
    log_info(f"Match coverage: {result.get('match_coverage', 0):.2%}")
    log_info("-" * 60)

    by_type = {}
    for m in matches:
        by_type[m.get("match_type", "?")] = by_type.get(m.get("match_type", "?"), 0) + 1
    for match_type, count in sorted(by_type.items()):
        log_info(f"  {match_type}: {count}")

    for i, m in enumerate(matches[:25]):
        fa, fb = m.get("function_a", {}), m.get("function_b", {})
        log_info(
            f"{i+1:3d}. {fa.get('name')} <-> {fb.get('name')}  "
            f"sim={m.get('similarity', 0):.3f} conf={m.get('confidence', 0):.3f} "
            f"[{m.get('match_type')}]"
        )
    if len(matches) > 25:
        log_info(f"  ... and {len(matches) - 25} more (see GUI/export)")
    log_info("=" * 60)


def run_binary_diff(bv):
    """Main function to run binary diffing"""
    if load_rust_engine() is None:
        return

    target_file = get_open_filename_input("Select target binary for comparison", "*.bndb")
    if not target_file:
        return

    try:
        target_bv = bn.load(target_file)
        if not target_bv:
            log_error(f"Failed to load target binary: {target_file}")
            return

        log_info(f"Starting diff between {bv.file.filename} and {target_bv.file.filename}")

        def _on_complete(result):
            # Runs on the background task thread; do log work here and marshal
            # any GUI work to the main thread.
            if not result:
                log_error("Binary diff did not produce a result")
                return

            if not result.get("matched_functions"):
                log_info("No function matches found; showing unmatched results")

            _log_summary(result)

            if HAS_GUI:
                def _show():
                    try:
                        window = show_diff_results(result, bv, target_bv)
                        if window:
                            log_info("Qt GUI window opened for detailed results")
                        else:
                            log_error("Failed to create Qt GUI window")
                    except Exception as e:
                        log_error(f"Failed to show GUI: {e}")
                bn.execute_on_main_thread(_show)
            else:
                log_info("Qt GUI not available. Install PySide6 for enhanced UI features.")

        # Fire-and-forget: the task reports completion via the callback so the
        # calling (UI) thread is never blocked on join().
        diff_task = BinaryDiffTask(bv, target_bv, on_complete=_on_complete)
        diff_task.start()

    except Exception as e:
        log_error(f"Error during binary diffing: {e}")


# Register the plugin command
try:
    PluginCommand.register(
        "Rust Diff\\Binary Diffing",
        "Compare functions between two BNDB files",
        run_binary_diff
    )
    log_info("Rust Diff Binary Diffing plugin loaded successfully")
except Exception as e:
    log_error(f"Failed to register Rust Diff Binary Diffing plugin: {e}")
