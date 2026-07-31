"""End-to-end test of the Rust diff engine via the JSON FFI.

Runs outside Binary Ninja: builds two small synthetic binaries in the
FunctionInfo JSON shape and checks match types, determinism, and that
unrelated leaf functions are not force-matched.

Usage: cargo build --release && python3 test_ffi.py
"""
import ctypes
import hashlib
import json
import os
import platform

PLUGIN_DIR = os.path.dirname(os.path.abspath(__file__))
LIB_NAME = {"Darwin": "librust_diff.dylib", "Linux": "librust_diff.so",
            "Windows": "rust_diff.dll"}[platform.system()]

lib = ctypes.CDLL(os.path.join(PLUGIN_DIR, "target", "release", LIB_NAME))
lib.rust_diff_diff_json.argtypes = [ctypes.c_char_p, ctypes.c_char_p]
lib.rust_diff_diff_json.restype = ctypes.c_void_p
lib.rust_diff_free_string.argtypes = [ctypes.c_void_p]
lib.rust_diff_il_diff_json.argtypes = [ctypes.c_char_p]
lib.rust_diff_il_diff_json.restype = ctypes.c_void_p


def bb(addr, mnems, edges):
    return {
        "address": addr, "size": len(mnems) * 4,
        "instructions": [
            {"address": addr + i * 4, "mnemonic": m,
             "operands": ["0x10"] if m == "cmp" else ["rax"],
             "bytes": [], "length": 4}
            for i, m in enumerate(mnems)
        ],
        "edges": edges,
        "mnemonic_hash": hashlib.sha256(" ".join(mnems).encode()).hexdigest()[:16],
        "instruction_count": len(mnems),
    }


def func(name, addr, blocks, callees=(), callers=()):
    instrs = [i for b in blocks for i in b["instructions"]]
    edges = sum(len(b["edges"]) for b in blocks)
    return {
        "name": name, "address": addr, "size": sum(b["size"] for b in blocks),
        "basic_blocks": blocks, "instructions": instrs,
        "cyclomatic_complexity": max(1, edges - len(blocks) + 2),
        "call_graph_hash": "", "cfg_hash": "",
        "instruction_count": len(instrs), "call_count": len(callees),
        "callees": list(callees), "callers": list(callers),
    }


# Binary A
a = [
    func("main", 0x1000, [
        bb(0x1000, ["push", "mov", "call"], [0x1010]),
        bb(0x1010, ["cmp", "jne"], [0x1020, 0x1030]),
        bb(0x1020, ["mov", "ret"], []),
        bb(0x1030, ["xor", "ret"], []),
    ], callees=[0x2000, 0x3000]),
    # identical name, modified body -> Name match
    func("helper", 0x2000, [bb(0x2000, ["push", "mov", "add", "ret"], [])], callers=[0x1000]),
    # stripped, body slightly modified, called by main -> MdIndex/CallGraph
    func("sub_3000", 0x3000, [
        bb(0x3000, ["push", "mov", "call"], [0x3010]),
        bb(0x3010, ["sub", "mul", "pop", "ret"], []),
    ], callers=[0x1000]),
    # unrelated tiny leaf, must NOT match only_in_b
    func("only_in_a", 0x4000, [bb(0x4000, ["nop", "nop", "ret"], [])]),
]

# Binary B: same program at different addresses
b = [
    func("main", 0x5000, [
        bb(0x5000, ["push", "mov", "call"], [0x5010]),
        bb(0x5010, ["cmp", "jne"], [0x5020, 0x5030]),
        bb(0x5020, ["mov", "ret"], []),
        bb(0x5030, ["xor", "ret"], []),
    ], callees=[0x6000, 0x7000]),
    func("helper", 0x6000, [bb(0x6000, ["push", "mov", "sub", "add", "ret"], [])], callers=[0x5000]),
    func("sub_7000", 0x7000, [
        bb(0x7000, ["push", "mov", "call"], [0x7010]),
        bb(0x7010, ["sub", "mul", "nop", "pop", "ret"], []),
    ], callers=[0x5000]),
    func("only_in_b", 0x8000, [bb(0x8000, ["int3", "ret"], [])]),
]

A, B = json.dumps(a).encode(), json.dumps(b).encode()


def run():
    ptr = lib.rust_diff_diff_json(A, B)
    assert ptr, "engine returned null"
    s = ctypes.cast(ptr, ctypes.c_char_p).value.decode()
    lib.rust_diff_free_string(ptr)
    r = json.loads(s)
    r.pop("analysis_time", None)  # wall clock, legitimately varies
    return r


r1, r2 = run(), run()
assert json.dumps(r1, sort_keys=True) == json.dumps(r2, sort_keys=True), "non-deterministic!"

for m in r1["matched_functions"]:
    fa, fb = m["function_a"], m["function_b"]
    print(f"{fa['name']:>10} <-> {fb['name']:<10} sim={m['similarity']:.3f} "
          f"conf={m['confidence']:.3f} type={m['match_type']}")
print("unmatched A:", [f["name"] for f in r1["unmatched_functions_a"]])
print("unmatched B:", [f["name"] for f in r1["unmatched_functions_b"]])

names = {(m["function_a"]["name"], m["function_b"]["name"]): m["match_type"]
         for m in r1["matched_functions"]}
assert names[("main", "main")] == "Exact", names
assert names[("helper", "helper")] == "Name", names
assert names.get(("sub_3000", "sub_7000")) in ("MdIndex", "Structural", "CallGraph"), names
assert ("only_in_a", "only_in_b") not in names, "unrelated leaf functions wrongly matched"
# slimmed payload still carries counts for the UI
fa = r1["matched_functions"][0]["function_a"]
assert fa["instructions"] == [] and fa["instruction_count"] > 0
assert 0 < r1["match_coverage"] < 1
assert r1["similarity_score"] < r1["matched_similarity_score"]


# Same mnemonic sequence with a changed operand is not exact.
operand_a = func("sub_9000", 0x9000, [bb(0x9000, ["mov", "ret"], [])])
operand_b = func("sub_a000", 0xA000, [bb(0xA000, ["mov", "ret"], [])])
operand_a["instructions"][0]["operands"] = ["eax", "1"]
operand_a["basic_blocks"][0]["instructions"][0]["operands"] = ["eax", "1"]
operand_b["instructions"][0]["operands"] = ["eax", "2"]
operand_b["basic_blocks"][0]["instructions"][0]["operands"] = ["eax", "2"]
ptr = lib.rust_diff_diff_json(json.dumps([operand_a]).encode(), json.dumps([operand_b]).encode())
operand_result = json.loads(ctypes.cast(ptr, ctypes.c_char_p).value.decode())
lib.rust_diff_free_string(ptr)
assert not operand_result["matched_functions"], operand_result


# ---------------------------------------------------------------------------
# Semantic IL diff: a renamed callee must read as cosmetic, a replaced callee
# as a real change (the distinction a textual diff cannot make).
# ---------------------------------------------------------------------------
def il_line(tokens, text):
    return {"tokens": [{"kind": k, "text": t} for k, t in tokens], "text": text}


_OLD = {"level": "HLIL", "lines": [
    il_line([("keyword", "if"), ("text", "("), ("keyword", "true"), ("text", ")")], "if (true)"),
    il_line([("codeSymbol", "testa"), ("text", "("), ("localVariable", "arg1"), ("text", ")")], "testa(arg1)"),
]}
_NEW = {"level": "HLIL", "lines": [
    il_line([("keyword", "if"), ("text", "("), ("keyword", "false"), ("text", ")")], "if (false)"),
    il_line([("codeSymbol", "testb"), ("text", "("), ("localVariable", "arg1"), ("text", ")")], "testb(arg1)"),
]}


def il_diff(req):
    ptr = lib.rust_diff_il_diff_json(json.dumps(req).encode())
    assert ptr, "il diff returned null"
    s = ctypes.cast(ptr, ctypes.c_char_p).value.decode()
    lib.rust_diff_free_string(ptr)
    return json.loads(s)


def op_for(diff, a_text):
    return next(l["op"] for l in diff["lines"] if l.get("a") == a_text)


# No rename map: replaced call is a real change.
d_replace = il_diff({"a": _OLD, "b": _NEW, "rename_map": {}})
assert op_for(d_replace, "testa(arg1)") == "replace", d_replace

# Matched-callee rename: the call is cosmetic, only the condition flip remains.
d_rename = il_diff({"a": _OLD, "b": _NEW, "rename_map": {"testa": "testb"}})
assert op_for(d_rename, "testa(arg1)") == "rename", d_rename
assert sum(1 for l in d_rename["lines"] if l["op"] == "replace") == 1, d_rename
print("IL diff: rename-vs-replace distinction OK")

# Bounds are semantic, not cosmetic immediates.
d_bound = il_diff({
    "a": {"level": "MLIL", "lines": [il_line([
        ("localVariable", "len"), ("text", " > "), ("integer", "64")], "len > 64")]},
    "b": {"level": "MLIL", "lines": [il_line([
        ("localVariable", "len"), ("text", " > "), ("integer", "128")], "len > 128")]},
})
assert d_bound["lines"][0]["op"] == "replace", d_bound
assert d_bound["similarity"] == 0.0, d_bound

print("\nAll assertions passed. Deterministic across runs: yes")
