#!/bin/sh
set -eu
dir=$1
kind=$2
mkdir -p "$dir"
git -C "$dir" init -q -b main
git -C "$dir" config user.name fixture
git -C "$dir" config user.email fixture
cat > "$dir/calc.py" <<'PY'
def add(a, b):
    return a + b

def divide(a, b):
    if b == 0:
        raise ValueError("division by zero")
    return a / b
PY
cat > "$dir/test_calc.py" <<'PY'
import unittest
from calc import add, divide

class CalcTests(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(3, 2), 5)
    def test_divide_by_zero(self):
        with self.assertRaises(ValueError):
            divide(3, 0)
PY
git -C "$dir" add calc.py test_calc.py
git -C "$dir" commit -q -m 'initial fixture'
case "$kind" in
 review)
    git -C "$dir" checkout -q -b feature
    cat > "$dir/calc.py" <<'PY'
def add(a, b):
    return a + b

def duplicate_add(a, b):
    return a + b

def divide(a, b):
    return a // b
PY
    ;;
 failing)
    cat > "$dir/calc.py" <<'PY'
def add(a, b):
    return a - b

def divide(a, b):
    return a / b
PY
    ;;
 *) exit 2 ;;
esac
git -C "$dir" add calc.py
git -C "$dir" commit -q -m 'change fixture'
printf '%s\n' "$dir"
