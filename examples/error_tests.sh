#!/bin/bash
# ALGOL26 Error Handling Test Suite
# Tests compile-time error detection with line/column info

echo "=== ALGOL26 ERROR HANDLING TESTS ==="
echo ""

echo "Test 1: Undefined variable"
cat > /tmp/error1.gol << 'GOL'
procedure main
    val x := 42.0
    print(undefined_var)
GOL
target/debug/algol26 check /tmp/error1.gol 2>&1
echo ""

echo "Test 2: Type mismatch"
cat > /tmp/error2.gol << 'GOL'
procedure main
    val x := "hello" + 5.0
GOL
target/debug/algol26 check /tmp/error2.gol 2>&1
echo ""

echo "Test 3: Use after move"
cat > /tmp/error3.gol << 'GOL'
procedure main
    val a := 42.0
    val b := a
    print(a)
GOL
target/debug/algol26 check /tmp/error3.gol 2>&1
echo ""

echo "Test 4: Bounds check"
cat > /tmp/error4.gol << 'GOL'
procedure main
    val arr := [1.0, 2.0, 3.0]
    print(arr[10])
GOL
target/debug/algol26 check /tmp/error4.gol 2>&1
echo ""

echo "Test 5: Immutability"
cat > /tmp/error5.gol << 'GOL'
procedure main
    val x := 10.0
    x := 20.0
GOL
target/debug/algol26 check /tmp/error5.gol 2>&1
echo ""

echo "=== ERROR TESTS COMPLETE ==="
