#!/usr/bin/env nu
# Issue 0016 acceptance: the Dual Input Pattern in the Nushell module —
# pipeline form and argument form produce identical values across the
# wrapper shapes, and the CLI's arity errors surface through the module.
# Usage: PATH must contain `ahtch`; run: nu scripts/test-dual-input.nu

use ../ahtch.nu *

$env.TMPDIR = (mktemp -d)
mut failed = false

def check [name: string, ok: bool] {
  print $"(if $ok { 'ok  ' } else { 'FAIL' }) ($name)"
  $ok
}

ahtch manual_seed 42 | ignore
let a = ([1 2 3] | ahtch tensor)
let b = ([4 5 6] | ahtch tensor)

# add (two tensors + flag)
let p1 = ($a | ahtch add $b | ahtch value)
let p2 = (ahtch add $a $b | ahtch value)
if not (check "add: both forms identical" ($p1 == $p2)) { $failed = true }
let f1 = ($a | ahtch add $b --alpha 2 | ahtch value)
let f2 = (ahtch add $a $b --alpha 2 | ahtch value)
if not (check "add --alpha: both forms identical" ($f1 == $f2)) { $failed = true }

# mm (two 2-D tensors)
let m = ([[1.0 2.0] [3.0 4.0]] | ahtch tensor)
let mm1 = ($m | ahtch mm $m | ahtch value)
let mm2 = (ahtch mm $m $m | ahtch value)
if not (check "mm: both forms identical" ($mm1 == $mm2)) { $failed = true }

# mse_loss (two tensors)
let t = ([1.5 2.5 3.5] | ahtch tensor)
let l1 = ([1.0 2.0 3.0] | ahtch tensor | ahtch mse_loss $t | ahtch value)
let l2 = (ahtch mse_loss ([1.0 2.0 3.0] | ahtch tensor) $t | ahtch value)
if not (check "mse_loss: both forms identical" ($l1 == $l2)) { $failed = true }

# zero_grad (single tensor, result nothing — parity via the grad read)
let w1 = (ahtch randn [3] --requires_grad)
let w2 = (ahtch randn [3] --requires_grad)
($w1 | ahtch mul $w1 | ahtch sum) | ahtch backward
($w2 | ahtch mul $w2 | ahtch sum) | ahtch backward
$w1 | ahtch zero_grad
ahtch zero_grad $w2
let g1 = ($w1 | ahtch grad | ahtch value)
let g2 = ($w2 | ahtch grad | ahtch value)
if not (check "zero_grad: both forms zero the grad" ($g1 == $g2 and ($g1 | math sum) == 0.0)) { $failed = true }

# gather (two tensors, --dim flag)
let src = ([[1.0 2.0] [3.0 4.0]] | ahtch tensor)
let idx = ([[0 0] [1 0]] | ahtch tensor --dtype int64)
let ga1 = ($src | ahtch gather $idx --dim 1 | ahtch value)
let ga2 = (ahtch gather $src $idx --dim 1 | ahtch value)
if not (check "gather --dim: both forms identical" ($ga1 == $ga2)) { $failed = true }

# reshape (tensor + IntList positional — the list-conversion path)
let r1 = ($a | ahtch reshape [3 1] | ahtch value)
let r2 = (ahtch reshape $a [3 1] | ahtch value)
if not (check "reshape [3 1]: both forms identical" ($r1 == $r2)) { $failed = true }

# cat (variadic — the untouched AtLeast arm, both forms)
let c1 = ([$a $b] | ahtch cat | ahtch value)
let c2 = (ahtch cat $a $b | ahtch value)
if not (check "cat: both forms identical" ($c1 == $c2)) { $failed = true }

# forward (prelude verb)
let model = (ahtch nn linear 3 2)
let fw1 = ($a | ahtch forward $model | ahtch value)
let fw2 = (ahtch forward $model $a | ahtch value)
if not (check "forward: both forms identical" ($fw1 == $fw2)) { $failed = true }

# tensor (prelude verb): data as argument or pipe — one encode path.
# Non-finite parity compares `to nuon` strings: nu 0.113's `==` is broken
# for inf/NaN ([inf 2.0] == [1.5 2.0] is true; [NaN] == [NaN] is false).
let tp = ([1.5 2.5] | ahtch tensor | ahtch value | to nuon)
let ta = (ahtch tensor [1.5 2.5] | ahtch value | to nuon)
if not (check "tensor: both forms identical" ($tp == $ta)) { $failed = true }
let nfp = ([inf 2.0] | ahtch tensor | ahtch value | to nuon)
let nfa = (ahtch tensor [inf 2.0] | ahtch value | to nuon)
if not (check "tensor non-finite: both forms identical (nuon)" ($nfp == $nfa and ($nfp | str contains "inf"))) { $failed = true }

# value (prelude verb): handle as argument or pipe.
let vh = ([7 8 9] | ahtch tensor)
let vp = ($vh | ahtch value | to nuon)
let va = (ahtch value $vh | to nuon)
if not (check "value: both forms identical" ($vp == $va)) { $failed = true }

# shape (prelude verb): handle as argument or pipe.
let sh = ([[1 2 3] [4 5 6]] | ahtch tensor)
let sp = ($sh | ahtch shape)
let sa = (ahtch shape $sh)
if not (check "shape: both forms identical" ($sp == $sa and $sp == [2 3])) { $failed = true }

# arity errors surface from the CLI (captured via a sub-shell: a def-internal
# external failure raises past `do | complete` in-process). Under-supply with
# non-TTY stdin reads EOF, so the CLI says "expected N piped handle(s), got 0";
# at a terminal it says "missing tensor operand(s)" — both are the grammar's.
let modpath = ($env.FILE_PWD | path join ".." | path join "ahtch.nu" | path expand)
let under = (do { ^nu -c $"use ($modpath) *; ahtch add" } | complete)
if not (check "under-supply names the CLI error" ($under.exit_code != 0 and (($under.stderr | str contains "piped handle") or ($under.stderr | str contains "missing tensor operand")))) { $failed = true }
let over = (do { ^nu -c $"use ($modpath) *; let t = \([1] | ahtch tensor\); ahtch add $t $t $t" } | complete)
if not (check "too many positionals names the CLI error" ($over.exit_code != 0 and ($over.stderr | str contains "too many arguments"))) { $failed = true }

ahtch daemon stop | ignore

if $failed { error make { msg: "dual-input parity failed" } }
print "PASS: dual input parity (nushell module)"
