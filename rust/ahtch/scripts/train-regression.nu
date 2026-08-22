#!/usr/bin/env nu
# Issue 0010: the Nushell twin of train-regression.sh — fit y = 2x + 1
# with linear(1,1) + SGD, natively. Private socket (TMPDIR) + seeded init.
# Usage: PATH must contain `ahtch`; run: nu scripts/train-regression.nu

use ../ahtch.nu *

$env.TMPDIR = (mktemp -d)

ahtch manual_seed 42 | ignore
let x = ([[0.0] [1.0] [2.0] [3.0]] | ahtch tensor)
let y = ([[1.0] [3.0] [5.0] [7.0]] | ahtch tensor)   # y = 2x + 1
let model = (ahtch nn linear 1 1)
let opt = (ahtch nn sgd $model --lr 0.05)

mut first_loss = -1.0
mut final_loss = -1.0
for i in 1..200 {
  let loss = ($x | ahtch forward $model | ahtch mse_loss $y)
  let v = ($loss | ahtch value)
  if $first_loss < 0 { $first_loss = $v }
  $final_loss = $v
  $loss | ahtch backward
  $opt | ahtch step
  ahtch nn zero_grad $opt | ignore
}

let params = (ahtch nn parameters $model | lines)
let weight = ($params | first | ahtch value | get 0.0)
let bias = ($params | get 1 | ahtch value | first)
print $"first loss: ($first_loss)"
print $"final loss: ($final_loss)"
print $"learned weight: ($weight) target 2, bias: ($bias) target 1"

ahtch daemon stop | ignore

if $final_loss >= 1e-3 { error make { msg: $"final loss ($final_loss) >= 1e-3" } }
if (($weight - 2.0) | math abs) / 2.0 >= 0.05 { error make { msg: $"weight ($weight) not within 5% of 2" } }
if (($bias - 1.0) | math abs) >= 0.05 { error make { msg: $"bias ($bias) not within 5% of 1" } }
print "PASS: regression fit y = 2x + 1 (nushell)"
