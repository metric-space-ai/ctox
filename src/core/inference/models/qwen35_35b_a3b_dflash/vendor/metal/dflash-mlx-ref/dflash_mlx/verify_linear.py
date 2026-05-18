# Copyright 2026 bstnxbt
# MIT License — see LICENSE file
# Based on DFlash (arXiv:2602.06036)
from __future__ import annotations

import os
from typing import Callable, Optional

import mlx.core as mx
import mlx.nn as nn
from mlx.utils import tree_map_with_path

from dflash_mlx.verify_qmm import verify_matmul


_VERIFY_MAX_N_DEFAULT = 100_000


def _env_int(name: str, default: int) -> int:
    try:
        return int(os.environ.get(name, str(default)))
    except ValueError:
        return default


_PROJ_TAGS = {
    "mlp.gate_proj":        "mlp_gate",
    "mlp.up_proj":          "mlp_up",
    "mlp.down_proj":        "mlp_down",
    "self_attn.q_proj":     "attn_q",
    "self_attn.k_proj":     "attn_k",
    "self_attn.v_proj":     "attn_v",
    "self_attn.o_proj":     "attn_o",
    "linear_attn.in_proj_qkv": "gdn_qkv",
    "linear_attn.in_proj_z":   "gdn_z",
    "linear_attn.out_proj":    "gdn_o",
}


def _path_tag(path: str) -> str:
    for suffix, tag in _PROJ_TAGS.items():
        if path.endswith(suffix):
            return tag
    return "other"


def is_verify_eligible(ql: nn.QuantizedLinear, path: str = "") -> bool:
    if not isinstance(ql, nn.QuantizedLinear):
        return False
    if getattr(ql, "bits", None) != 4:
        return False
    if getattr(ql, "group_size", None) not in (32, 64, 128):
        return False
    if getattr(ql, "mode", "affine") != "affine":
        return False
    w = ql.weight
    N = int(w.shape[0])
    K = int(w.shape[1]) * (32 // ql.bits)
    if N % 32 != 0 or K % 32 != 0:
        return False
    if N >= _env_int("DFLASH_VERIFY_MAX_N", _VERIFY_MAX_N_DEFAULT):
        return False
    include = os.environ.get("DFLASH_VERIFY_INCLUDE", "all").strip().lower()
    if include not in ("", "all"):
        tag = _path_tag(path)
        allowed = {s.strip() for s in include.split(",") if s.strip()}
        if "mlp" in allowed:
            allowed.update({"mlp_gate", "mlp_up", "mlp_down"})
        if "attn" in allowed:
            allowed.update({"attn_q", "attn_k", "attn_v", "attn_o"})
        if "gdn" in allowed:
            allowed.update({"gdn_qkv", "gdn_z", "gdn_o"})
        if tag not in allowed:
            return False
    return True


class VerifyQuantizedLinear(nn.QuantizedLinear):

    @classmethod
    def from_quantized(cls, ql: nn.QuantizedLinear) -> "VerifyQuantizedLinear":
        obj = cls.__new__(cls)
        nn.Module.__init__(obj)
        obj.group_size = ql.group_size
        obj.bits = ql.bits
        obj.mode = getattr(ql, "mode", "affine")
        obj.weight = ql.weight
        obj.scales = ql.scales
        if getattr(ql, "biases", None) is not None:
            obj.biases = ql.biases
        if "bias" in ql:
            obj.bias = ql.bias

        object.__setattr__(obj, "_call_fn", _build_dispatch(obj))

        obj.freeze()
        return obj

    def __call__(self, x: mx.array) -> mx.array:
        return self._call_fn(x)


def _build_dispatch(obj: "VerifyQuantizedLinear"):
    w = obj.weight
    s = obj.scales
    b = getattr(obj, "biases", None)
    gs = obj.group_size
    bits = obj.bits
    mode = obj.mode
    has_bias = "bias" in obj
    bias = obj.bias if has_bias else None

    if has_bias:
        def call(x):
            m = 1
            for d in x.shape[:-1]:
                m *= d
            if m == 16:
                y = verify_matmul(
                    x, w, s, b,
                    transpose=True, group_size=gs, bits=bits,
                )
            else:
                y = mx.quantized_matmul(
                    x, w, scales=s, biases=b,
                    transpose=True, group_size=gs, bits=bits, mode=mode,
                )
            return y + bias
    else:
        def call(x):
            m = 1
            for d in x.shape[:-1]:
                m *= d
            if m == 16:
                return verify_matmul(
                    x, w, s, b,
                    transpose=True, group_size=gs, bits=bits,
                )
            return mx.quantized_matmul(
                x, w, scales=s, biases=b,
                transpose=True, group_size=gs, bits=bits, mode=mode,
            )
    return call


def install_verify_linears(
    model: nn.Module,
    *,
    predicate: Optional[Callable[[str, nn.QuantizedLinear], bool]] = None,
) -> int:
    if predicate is None:
        predicate = lambda path, m: is_verify_eligible(m, path=path)

    count = 0

    def _maybe_swap(path, m):
        nonlocal count
        if isinstance(m, VerifyQuantizedLinear):
            return m
        if isinstance(m, nn.QuantizedLinear) and predicate(path, m):
            count += 1
            return VerifyQuantizedLinear.from_quantized(m)
        return m

    leaves = model.leaf_modules()
    leaves = tree_map_with_path(_maybe_swap, leaves, is_leaf=nn.Module.is_module)
    model.update_modules(leaves)
    return count


def uninstall_verify_linears(model: nn.Module) -> int:
    count = 0

    def _maybe_unswap(path, m):
        nonlocal count
        if isinstance(m, VerifyQuantizedLinear):
            ql = nn.QuantizedLinear.__new__(nn.QuantizedLinear)
            nn.Module.__init__(ql)
            ql.group_size = m.group_size
            ql.bits = m.bits
            ql.mode = m.mode
            ql.weight = m.weight
            ql.scales = m.scales
            if getattr(m, "biases", None) is not None:
                ql.biases = m.biases
            if "bias" in m:
                ql.bias = m.bias
            ql.freeze()
            count += 1
            return ql
        return m

    leaves = model.leaf_modules()
    leaves = tree_map_with_path(_maybe_unswap, leaves, is_leaf=nn.Module.is_module)
    model.update_modules(leaves)
    return count
