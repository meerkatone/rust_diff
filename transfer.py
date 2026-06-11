"""Analysis transfer (A -> B).

Given matched functions from the diff, copy analysis information from binary A
onto binary B: the function name, its prototype/type, and comments. Unlike a
passive WARP-style transfer, both binaries are open, so the user picks exactly
which attributes to apply and the whole batch is one undo action.

This module is pure Python over the live BinaryViews; the Rust engine is not
involved. It is import-safe outside Binary Ninja (the bn import is lazy).
"""

from typing import Dict, List, Optional


def _is_auto_generated_name(name: str) -> bool:
    """Mirror of the Rust matcher's placeholder-name test (matching.rs): never
    propagate an auto-generated name over a real one."""
    if not name:
        return True
    n = name[2:] if name.startswith("j_") else name
    prefixes = ("sub_", "SUB_", "FUN_", "fun_", "loc_", "fcn.", "func_")
    return n.startswith(prefixes) or n == "unnamed"


# Attribute keys used in options dicts and the preview.
ATTR_NAME = "name"
ATTR_PROTOTYPE = "prototype"
ATTR_COMMENTS = "comments"
ATTR_VARIABLES = "variables"
ALL_ATTRS = (ATTR_NAME, ATTR_PROTOTYPE, ATTR_COMMENTS, ATTR_VARIABLES)


import re

# Binary Ninja auto-generated variable names: var_18, var_c8_1, arg1, arg_8,
# and bare registers (rax, edi, r8, xmm0, al, …). None carry analyst intent.
# A `_N` SSA/dup suffix is allowed on any of these. Meaningful short names like
# dst/src/buf/len are deliberately NOT matched.
_AUTO_VAR_RE = re.compile(
    r"^("
    r"var_[0-9a-f]+|"
    r"arg_?\d+|"
    r"[re]?[abcd]x|[re]?[sd]i|[re]?[bs]p|r\d{1,2}[dwb]?|[abcd][lh]|"
    r"[xyz]mm\d+"
    r")(_\d+)?$"
)


def _is_auto_var_name(name: str) -> bool:
    return not name or bool(_AUTO_VAR_RE.match(name))


def _var_key(var):
    """Stable identity for a variable across two builds of the same function:
    (source_type, storage). Good for unchanged/lightly-changed functions; a
    mismatch simply means no transfer for that variable."""
    try:
        return (int(var.source_type), int(var.storage))
    except Exception:
        return None


def _plan_variables(func_a, func_b):
    """Propose variable name/type changes B should adopt from A, paired by
    storage. Returns a list of {attr:'variable', old, new, _key}."""
    changes = []
    try:
        vars_a = list(func_a.vars)
        b_by_key = {}
        for vb in func_b.vars:
            k = _var_key(vb)
            if k is not None:
                b_by_key[k] = vb
    except Exception:
        return changes

    for va in vars_a:
        k = _var_key(va)
        if k is None or k not in b_by_key:
            continue
        vb = b_by_key[k]
        new_name = None
        new_type = None
        # Name: only copy a meaningful (analyst-given) name over a different one.
        if not _is_auto_var_name(va.name) and va.name != vb.name:
            new_name = va.name
        # Type: copy when it differs.
        try:
            if str(va.type) != str(vb.type):
                new_type = va.type
        except Exception:
            pass
        if new_name is None and new_type is None:
            continue
        desc_old = f"{vb.name}: {vb.type}"
        desc_new = f"{new_name or vb.name}: {new_type if new_type is not None else vb.type}"
        changes.append({"attr": ATTR_VARIABLES, "old": desc_old, "new": desc_new,
                        "_key": k, "_new_name": new_name, "_new_type": new_type})
    return changes


def _resolve_funcs(bv_a, bv_b, match):
    """Resolve the live A/B function objects for one match dict, or (None, None)."""
    addr_a = match.get("function_a", {}).get("address")
    addr_b = match.get("function_b", {}).get("address")
    if addr_a is None or addr_b is None:
        return None, None
    try:
        return bv_a.get_function_at(addr_a), bv_b.get_function_at(addr_b)
    except Exception:
        return None, None


def plan_transfer(bv_a, bv_b, matches: List[dict], options: Dict[str, bool]) -> List[dict]:
    """Compute the changes a transfer would make, without applying them. Returns
    a list of per-function plans: {addr_b, from, changes: [{attr, old, new}]}.
    `options` enables/disables each attribute (ATTR_* keys)."""
    plans = []
    for match in matches:
        func_a, func_b = _resolve_funcs(bv_a, bv_b, match)
        if not func_a or not func_b:
            continue

        changes = []

        if options.get(ATTR_NAME) and not _is_auto_generated_name(func_a.name):
            if func_a.name != func_b.name:
                changes.append({"attr": ATTR_NAME, "old": func_b.name, "new": func_a.name})

        if options.get(ATTR_PROTOTYPE):
            try:
                type_a, type_b = str(func_a.type), str(func_b.type)
                if type_a and type_a != type_b:
                    # Carry the Type *object* (not its str): assigning a string
                    # to Function.type also parses a name out of it and renames
                    # the function — an unwanted side effect. set_user_type on
                    # the object changes only the type.
                    changes.append({"attr": ATTR_PROTOTYPE, "old": type_b, "new": type_a,
                                    "_new_type": func_a.type})
            except Exception:
                pass

        if options.get(ATTR_COMMENTS):
            try:
                comment_a = func_a.comment or ""
                if comment_a and comment_a != (func_b.comment or ""):
                    changes.append({"attr": ATTR_COMMENTS, "old": func_b.comment or "", "new": comment_a})
            except Exception:
                pass

        if options.get(ATTR_VARIABLES):
            changes.extend(_plan_variables(func_a, func_b))

        if changes:
            plans.append({
                "addr_b": func_b.start,
                "from": func_a.name,
                "to": func_b.name,
                "changes": changes,
            })
    return plans


def apply_transfer(bv_b, plans: List[dict]) -> dict:
    """Apply the planned changes to binary B inside a single undo action. Returns
    a summary {functions, attributes, errors}."""
    import binaryninja as bn  # lazy: only needed at apply time inside BN

    applied_funcs = 0
    applied_attrs = 0
    errors = []

    undo = None
    try:
        undo = bv_b.begin_undo_actions()
    except Exception:
        undo = None

    try:
        for plan in plans:
            func_b = bv_b.get_function_at(plan["addr_b"])
            if not func_b:
                continue
            # Resolve B variables by their (source_type, storage) key once.
            b_vars_by_key = {}
            if any(c["attr"] == ATTR_VARIABLES for c in plan["changes"]):
                for vb in func_b.vars:
                    k = _var_key(vb)
                    if k is not None:
                        b_vars_by_key[k] = vb
            touched = False
            for change in plan["changes"]:
                attr = change["attr"]
                try:
                    if attr == ATTR_NAME:
                        func_b.name = change["new"]
                    elif attr == ATTR_PROTOTYPE:
                        # Use the Type object to avoid the string setter's
                        # implicit rename side effect (see plan_transfer).
                        func_b.set_user_type(change["_new_type"])
                    elif attr == ATTR_COMMENTS:
                        func_b.comment = change["new"]
                    elif attr == ATTR_VARIABLES:
                        vb = b_vars_by_key.get(change["_key"])
                        if vb is None:
                            continue
                        new_name = change["_new_name"] if change["_new_name"] is not None else vb.name
                        new_type = change["_new_type"] if change["_new_type"] is not None else vb.type
                        func_b.create_user_var(vb, new_type, new_name)
                    applied_attrs += 1
                    touched = True
                except Exception as e:
                    errors.append(f"0x{plan['addr_b']:x} {attr}: {e}")
            if touched:
                applied_funcs += 1
    finally:
        try:
            if undo is not None:
                bv_b.commit_undo_actions(undo)
            else:
                bv_b.commit_undo_actions()
        except Exception:
            pass
        try:
            bv_b.update_analysis()
        except Exception:
            pass

    return {"functions": applied_funcs, "attributes": applied_attrs, "errors": errors}
